# コールバックの必須境界から Sync を削除する

- Created: 2026-10-27
- Completed: 2026-10-29
- Branch: feature/update-remove-callback-sync
- Polished: {YYYY-MM-DD}

## 目的

`AudioCapture::new` / `AudioPlayback::new` と各バックエンドのコンストラクタがコールバックに要求している `Sync` を削除し、`Send` だけを要求するようにする。

現在の `Sync` 要求は、コールバックがバックエンドの専用スレッドからしか呼ばれない実装に対して過剰である。`Sync` を外すことで、次のような `Send` だが `!Sync` な状態をコールバック内で扱えるようになる。

- `Cell<T>` / `RefCell<T>`: コールバック内だけで使うカウンタや作業バッファ
- `mpsc::Receiver<T>`: 別スレッドから送られたコマンドをコールバック内で消化するキュー
- `OnceCell<T>` など

現状、これらの型は `Sync` を満たさないため、`AtomicU64` や `Mutex` で包む必要がある。実際に `examples/playback_sine.rs` は位相保持に `AtomicU64` を使い、`tests/test_device_timing.rs` も `Arc<AtomicU64>` をコールバックに渡している。`Cell<u64>` すら使えないことが、コールバック実装を不要に複雑にしている。

`Sync` は「コールバックが並行に呼ばれること」を意味しない。「`&callback` を別スレッドへ渡してよいこと」を意味する。各バックエンドはコールバックを単一の専用スレッドから直列に呼び、`stop` は実行中コールバックの完了を待つため、`&callback` が同時に複数スレッドから使われることはない。この前提を契約として明文化すれば、`Send` だけで安全性を説明できる。

## 現状

### 公開 API

`src/capture.rs` の `AudioCapture::new` / `new_coreaudio` / `new_pulse` / `new_pipewire` / `new_wasapi` と、`src/playback.rs` の `AudioPlayback::new` / `new_coreaudio` / `new_pulse` / `new_pipewire` / `new_wasapi` は、コールバックに `F: Fn(...) + Send + Sync + 'static` を要求している。

### FFI バックエンド

`src/capture_ffi.rs` の `CaptureContext` と `src/playback_ffi.rs` の `PlaybackContext` は、コールバックを `Box<dyn Fn(...) + Send + Sync>` として保持している。C 側には `Box` のポインタを `user_data` として渡し、バックエンドのコールバック関数から `&CaptureContext` / `&PlaybackContext` を作って呼ぶ。`FfiCaptureImpl` / `FfiPlaybackImpl` は `unsafe impl Send` を持つが、`Sync` は持たない。FFI 側では `Sync` を型検査で要求している箇所はなく、安全性コメントで「コールバックコンテキストは `Send + Sync` な `Fn` だけを保持する」と宣言しているだけである。

### WASAPI

`src/capture_wasapi.rs` の `WasapiCaptureContext` と `src/playback_wasapi.rs` の `PlaybackContext` は、コールバックと `AtomicBool` の `running` を保持し、`Arc` で共有している。`start` は `Arc::clone` したコンテキストを `thread::Builder::spawn` のクロージャへ move する。`Arc<T>: Send` の条件は `T: Send + Sync` であるため、ここではコンパイラがコールバックの `Sync` を強制している。`Sync` を外すには、コールバックと `running` を分離し、コールバックをスレッド間で共有しない構造にする必要がある。

### バックエンドのスレッドモデル

- CoreAudio: `AudioQueueNewInput` / `AudioQueueNewOutput` の内部スレッド
- PulseAudio: `pa_threaded_mainloop` のスレッド
- PipeWire: `pw_thread_loop` のスレッド
- WASAPI: `src/capture_wasapi.rs` / `src/playback_wasapi.rs` が生成する専用スレッド

いずれもセッションごとに 1 本のスレッドでコールバックを直列に呼ぶ。`audio.h` の stop の契約には「コールバックのスレッド上から呼び出さない場合、戻るまでに実行中のユーザーコールバックの完了を待ち、戻った後にユーザーコールバックを呼ばない」とあるが、「コールバックは直列に呼ばれる」ということ自体は明記されていない。

`AudioCapture` / `AudioPlayback` は `Sync` ではなく、`AudioCaptureSession` は別スレッドへ move できる。`AudioDevice` / `AudioDeviceList` は read-only なので `Send + Sync` のままでよく、本 issue の対象外である。

## 設計方針

### 公開 API

コールバックの境界を `F: Fn(...) + Send + 'static` に変更する。`Sync` は要求しない。既存の `Fn` クロージャはそのまま渡せるため、利用者側のコードは変更不要である。

### FFI バックエンド

`CaptureContext` / `PlaybackContext` の trait object から `Sync` を削除し、`Box<dyn Fn(...) + Send>` にする。構造変更は不要である。`unsafe impl Send` の safety コメントを次の趣旨に更新する。

- コールバックは各バックエンドの専用スレッドからのみ呼ばれる
- `stop` は実行中コールバックの完了を待つため、`&callback` が同時に複数スレッドから使われることはない

`audio.h` のセッション API の共通仕様に「コールバックは単一のバックエンドスレッドから直列に呼ばれる」を追記し、Rust 側の safety コメントの根拠を C API の契約として明文化する。

### WASAPI

`Arc<WasapiCaptureContext>` / `Arc<PlaybackContext>` を次のように分離する。

- `running: Arc<AtomicBool>`: スレッドと共有する実行状態
- `context: Box<WasapiCaptureContext>` / `Box<PlaybackContext>`: コールバックだけを保持し、`self` に残す
- スレッドには `SendPtr<*mut WasapiCaptureContext>` / `SendPtr<*mut PlaybackContext>` と `Arc<AtomicBool>` を渡す
- `stop` は `running` を `false` にしてイベントをシグナルし、スレッドを `join` する。コンテキストは再 `start` のために `self` に残り、`WasapiCaptureImpl` / `WasapiPlaybackImpl` の drop 時に解放される
- `SendPtr<*mut ...>` に具体的な `unsafe impl Send` を付け、safety コメントに「コンテキストは Box が drop されるまで生存し、スレッドは join 後にアクセスしない」「コールバックは専用スレッドからのみ呼ばれる」と書く

この形なら、コールバックをどのスレッドとも共有しないため `Sync` は不要であり、`Mutex` も使わない。`thread::Builder::spawn` が失敗した場合もコンテキストは `self` に残るため、コールバックを失わない。

代替案として、video-device-rs の `MfCaptureImpl` と同じく、コールバックをスレッドへ move して `JoinHandle` で回収する方法がある。ただし audio の WASAPI は `thread::Builder::spawn` の失敗を `Error::SessionStartFailed` として扱っており、この方法では spawn 失敗時にコールバックが失われる。そのため、本 issue では raw pointer 方式を採用する。

### コールバックの型

`Fn` のまま変更しない。`FnMut` への変更は別の API 変更であり、本 issue の目的ではない。

## 完了条件

- `AudioCapture::new` / `AudioPlayback::new` と各バックエンドのコンストラクタのコールバック境界が `Send` のみになること
- `CaptureContext` / `PlaybackContext` / WASAPI のコンテキストから `Sync` が消えること
- `Cell` / `RefCell` / `mpsc::Receiver` のような `Send + !Sync` な状態を捕捉したコールバックがコンパイルできること
- `AudioCapture` / `AudioPlayback` が `Sync` でないこと、`AudioCaptureSession` が別スレッドへ move できることが維持されること
- `audio.h` と safety コメントに「コールバックは専用スレッドから直列に呼ばれる」ことが明記されること
- `cargo fmt --check` / `cargo clippy --workspace --all-targets` / `cargo test --workspace` が通ること
- `CHANGES.md` に [UPDATE] として記載すること

## テスト

- `tests/test_capture.rs` / `tests/test_playback.rs` に、`Cell<u64>` を捕捉したコールバックを渡すテストを追加する。デバイスが無い環境では `new` がエラーを返すため、`Ok` / `Err` のどちらでもテストを成功させる。このテストの目的は実行時の動作ではなく、コールバック境界のコンパイル可否であることをコメントに書く。
- `tests/session_lifecycle.rs` の「コールバックをブロックしたまま別スレッドから stop する」テストが引き続き通ることを確認する。
- WASAPI の raw pointer 経路はデバイスが必要なため、実機テストとコードレビューで確認する。

## 後方互換

コールバック境界の緩和は後方互換である。`Sync` を要求されていたコードは、`Send` だけの要求を満たすため、そのままコンパイルできる。`AudioCapture` / `AudioPlayback` の `Send` / `Sync` の性質は変わらない。

## 参考

video-device-rs では既にコールバックから `Sync` を外している。

- `src/capture_ffi.rs`: `CaptureContext` が `Box<dyn Fn(...) + Send>` を保持し、`unsafe impl Send` の safety コメントで「全バックエンドでキャプチャは専用スレッド上で動作する」と説明している
- `src/capture_mf.rs`: `MfCaptureImpl` が `running: Arc<AtomicBool>` と `callback: Option<VideoFrameCallback>` を分離し、コールバックをキャプチャスレッドへ move して `JoinHandle<VideoFrameCallback>` で回収している
- video-device-rs の commit `629a2f6` で `Sync` を削除し、`Arc<CaptureContext>` を `Box<CaptureContext>` に変更した
- video-device-rs の commit `45e83a0` で `JoinHandle<VideoFrameCallback>` による回収を追加した

これらの実装は、コールバックを複数スレッドで共有しないことを前提にしている。audio-device-rs でも同じ前提を `audio.h` と safety コメントで明文化する。

## 解決方法

- `AudioCapture::new` / `AudioPlayback::new` と各バックエンドのコンストラクタからコールバックの `Sync` 要求を削除し、`Send` のみを要求するようにした
- `CaptureContext` / `PlaybackContext` のコールバックの trait object から `Sync` を削除した
- WASAPI のコンテキストから `running` を分離し、`Box` のコンテキストと `Arc<AtomicBool>` の実行状態をスレッドで共有する構造に変更した
- `audio.h` と safety コメントに、コールバックがバックエンドの管理するスレッド上で直列に呼ばれることを明記した
- `tests/test_capture.rs` / `tests/test_playback.rs` に、`Cell` を捕捉したコールバックをデフォルトバックエンドへ渡すテストを追加した
- WASAPI でも NUL バイトを含むデバイス ID を `Error::InvalidDeviceId` として拒否するようにした
- `CHANGES.md` にコールバック境界の変更を [UPDATE] として記載した
- PR #23 で `develop` にマージされた
