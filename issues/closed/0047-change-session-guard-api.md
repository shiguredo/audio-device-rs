# セッション実行中を借用で表すガード API に変更する

- Created: 2026-10-27
- Completed: 2026-10-27
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

pub struct AudioCaptureSession<'a> {
    capture: &'a mut AudioCapture,
}

impl AudioCapture {
    pub fn start(&mut self) -> Result<AudioCaptureSession<'_>>;
}

impl AudioCaptureSession<'_> {
    pub fn stop(self);
}
```

- ガードが `&mut AudioCapture` を保持するため、セッション中は `AudioCapture` のメソッドを呼べない。実行中の破棄もコンパイルエラーになる
- `AudioCaptureSession::stop` は C の `audio_pipewire_session_stop` / `audio_pulse_session_stop` / `audio_coreaudio_session_stop` を呼ぶ。いずれもコールバックの完了を待ってからリソースを破棄するため、コールバック実行中に別スレッドから停止しても安全
- ガードの `Drop` でも停止する（停止忘れを防ぐ）
- 実行中に読み取りたい値（`config` / `sample_rate` / `channels`）はガードに転送する
- 実行中に別スレッドから停止する場合は、ガードをそのスレッドへ move して `stop` を呼ぶ
- 代替案として typestate（`fn start(self) -> Result<AudioCaptureSession>` / `fn stop(self) -> Result<AudioCapture>`）も検討する

この方式にすると次が不要になる。

- Rust 層と C API の再入ガード（`session_is_in_callback`）
- `stop()` の `Result` 化と `Error::SessionStopFailed`（再入エラーが存在しなくなる）
- `Drop` 時の use-after-free 経路（実行中の Drop がコンパイルエラーになる）

WASAPI も同じ形にする。`WasapiCaptureImpl` / `WasapiPlaybackImpl` はスレッドハンドルの ID 比較で再入を判定しているため、ガード化すればその判定自体が不要になる。

## 完了条件

- セッション実行中に `AudioCapture` / `AudioPlayback` を drop するとコンパイルエラーになること（compile_fail の doctest などで固定する）
- セッション実行中に `&mut AudioCapture` / `&mut AudioPlayback` を取得できないこと
- 実行中に別スレッドからガード経由で停止できること
- 既存の `examples/` と `tests/` が新 API に移行して通ること
- Rust 層と C API から `session_is_in_callback` を削除すること

## 解決方法

- `src/capture.rs` / `src/playback.rs` に `AudioCaptureSession` / `AudioPlaybackSession` を追加し、`AudioCapture::start` / `AudioPlayback::start` の戻り値をガードに変更した
- ガードは `&mut AudioCapture` / `&mut AudioPlayback` を保持し、`stop` と `Drop` で C の `*_session_stop` を呼ぶ。`config` / `sample_rate` / `channels` はガードから取得できるようにした
- `AudioCapture::stop` / `AudioPlayback::stop` を削除した
- `AudioCaptureSession::stop` / `AudioPlaybackSession::stop` は戻り値を持たない。再入に起因する停止失敗はガードにより発生しないため、`Error::SessionStopFailed` を削除した
- `src/capture_ffi.rs` / `src/playback_ffi.rs` から `session_is_in_callback` の参照を削除した
- C API の `*_session_is_in_callback` と `in_callback` 状態を全バックエンドから削除した。このクレートは C API を外部公開していないため、C 利用者向けの互換性は考慮しない
- `src/capture_wasapi.rs` / `src/playback_wasapi.rs` からスレッド ID 比較による再入判定と `Drop` 時の panic を削除した
- `audio.h` の stop の契約に「コールバックのスレッド上から呼び出さない場合、戻るまでに実行中のユーザーコールバックの完了を待ち、戻った後にユーザーコールバックを呼ばない」を追記した
- `examples/` と `tests/` を新しいガード API に移行し、`README.md` の使い方も更新した
- `AudioCapture::start` / `AudioPlayback::start` の doc に、セッション中の再 start と drop がコンパイルエラーになることを示す compile_fail doctest を追加した
- `tests/session_lifecycle.rs` に、コールバックをブロックしたまま別スレッドからガード経由で停止するテストを追加した

## 後方互換

公開 API の変更。`start` の戻り値が変わり、`AudioCapture::stop` / `AudioPlayback::stop` は戻り値を持たないガードの `stop` へ移る。`CHANGES.md` に [CHANGE] として記載する。
