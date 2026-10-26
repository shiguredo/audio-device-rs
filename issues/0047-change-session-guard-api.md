# セッション実行中を借用で表すガード API に変更する

- Created: 2026-10-27
- Completed: {YYYY-MM-DD}
- Branch: feature/change-session-guard-api
- Polished: {YYYY-MM-DD}

## 目的

`AudioCapture` / `AudioPlayback` のセッション API を、セッション実行中はオブジェクトを借用するガード方式へ変更する。

現行 API は `AudioCapture` を保持したまま `start` / `stop` を呼ぶ形で、コールバック実行中でも `&mut AudioCapture` を取得できる。そのため安全性を実行時フラグ `is_in_callback` に依存しており、次の 2 つの致命的な欠陥が生じている。

- `audio_coreaudio_session_is_in_callback` / `audio_coreaudio_playback_session_is_in_callback` がセッション単位の `atomic_int` を返しており、`audio.h` の「現在のスレッドがユーザーコールバックを実行中なら 1」という契約に反する。別スレッドからの `stop` が誤って `Error::SessionStopFailed` になり、`Drop` が誤って panic する
- `FfiCaptureImpl` / `FfiPlaybackImpl` の `Drop` が `panic!` すると `let _ = self.stop()` と `session_destroy` を飛ばすが、panic してもフィールドは drop されるため `CaptureContext` / `PlaybackContext` が解放される。C セッションは停止も破棄もされないまま `user_data` が解放済みポインタを参照し、use-after-free になる

セッション実行中にオブジェクトを借用すれば、実行中の破棄・二重 start・再入 stop をコンパイル時に排除でき、実行時フラグへの依存をなくせる。

## 現状

- `AudioCapture::start` / `AudioCapture::stop`（`src/capture.rs`）と `AudioPlayback::start` / `AudioPlayback::stop`（`src/playback.rs`）は `&mut self` を取るが、セッションの生存期間を型で表していない
- `FfiCaptureImpl::start`（`src/capture_ffi.rs`）は `CaptureContext` への raw pointer を C に渡すため、コールバック中のアクセスは borrow checker の管理外にある
- `FfiCaptureImpl::stop` / `FfiCaptureImpl::drop`（`src/capture_ffi.rs`）と `FfiPlaybackImpl::stop` / `FfiPlaybackImpl::drop`（`src/playback_ffi.rs`）が `session_is_in_callback` を参照して再入を判定している
- `CaptureOps` / `PlaybackOps` の `session_is_in_callback` は PipeWire / PulseAudio ではスレッド単位、CoreAudio ではセッション単位で実装されている
- `AudioCapture::stop` / `AudioPlayback::stop` が `Result<()>` を返す理由は、現状「コールバックからの再入」だけである

## 設計方針

`start` がガードを返し、ガードがセッションの生存期間を表す。

```rust
pub struct AudioCapture { /* ... */ }

pub struct CaptureSession<'a> {
    capture: &'a mut AudioCapture,
}

impl AudioCapture {
    pub fn start(&mut self) -> Result<CaptureSession<'_>>;
}

impl CaptureSession<'_> {
    pub fn stop(self) -> Result<()>;
}
```

- ガードが `&mut AudioCapture` を保持するため、セッション中は `AudioCapture` のメソッドを呼べない。実行中の破棄もコンパイルエラーになる
- `CaptureSession::stop` は C の `audio_pipewire_session_stop` / `audio_pulse_session_stop` / `audio_coreaudio_session_stop` を呼ぶ。いずれもコールバックの完了を待ってからリソースを破棄するため、コールバック実行中に別スレッドから停止しても安全
- ガードの `Drop` でも停止する（停止忘れを防ぐ）
- 実行中に読み取りたい値（`config` / `sample_rate` / `channels`）はガードに転送する
- 実行中に別スレッドから停止する場合は、ガードをそのスレッドへ move して `stop` を呼ぶ
- 代替案として typestate（`fn start(self) -> Result<CaptureSession>` / `fn stop(self) -> Result<AudioCapture>`）も検討する

この方式にすると次が不要になる。

- Rust 層の再入ガード（`session_is_in_callback` の参照）。C API の契約は C 利用者向けに残す
- `stop()` の `Result` 化と `Error::SessionStopFailed`（再入エラーが存在しなくなる）
- `Drop` 時の use-after-free 経路（実行中の Drop がコンパイルエラーになる）

WASAPI も同じ形にする。`WasapiCaptureImpl` / `WasapiPlaybackImpl` はスレッドハンドルの ID 比較で再入を判定しているため、ガード化すればその判定自体が不要になる。

## 完了条件

- セッション実行中に `AudioCapture` / `AudioPlayback` を drop するとコンパイルエラーになること（compile_fail の doctest などで固定する）
- セッション実行中に `&mut AudioCapture` / `&mut AudioPlayback` を取得できないこと
- 実行中に別スレッドからガード経由で停止できること
- 既存の `examples/` と `tests/` が新 API に移行して通ること
- Rust 層から `session_is_in_callback` の参照がなくなること

## 解決方法

1. `src/capture.rs` / `src/playback.rs` に `CaptureSession` / `PlaybackSession` を追加し、`AudioCapture::start` / `AudioPlayback::start` の戻り値をガードに変更する
2. ガードに `stop` と `Drop` を実装し、C の `*_session_stop` を呼ぶ。`config` / `sample_rate` / `channels` はガードへ転送する
3. `AudioCapture::stop` / `AudioPlayback::stop` を削除する（`CHANGES.md` に [CHANGE] として記載する）
4. `src/capture_ffi.rs` / `src/playback_ffi.rs` / `src/capture_wasapi.rs` / `src/playback_wasapi.rs` から `session_is_in_callback` の参照とスレッド ID 比較を削除する
5. `examples/` と `tests/` を新 API に移行する
6. 実行中の drop と `&mut` の取得がコンパイルエラーになることを compile_fail の doctest で固定する
7. C API の `*_session_is_in_callback` は C 利用者向けの契約として残し、全バックエンドでスレッド単位の実装に揃える

## 後方互換

公開 API の変更。`start` の戻り値が変わり、`AudioCapture::stop` / `AudioPlayback::stop` はガードへ移る。`CHANGES.md` に [CHANGE] として記載する。
