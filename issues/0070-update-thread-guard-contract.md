# ThreadGuard の契約を must_use とドキュメント、コンパイルテストで守る

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-thread-guard-contract
- Polished: 2026-11-04

## 目的

`ThreadGuard` の保持忘れと、`AudioCaptureSession` / `AudioPlaybackSession` を別スレッドへ move した際の初期化要件の抜けを防ぐ。あわせて、`ThreadGuard` をオーディオオブジェクトより先に drop した場合の COM 解放順序をドキュメントで明示する。

## 現状

- `src/runtime.rs` の `ThreadGuard` に `#[must_use]` が無い。`initialize_thread()?;` と書くと戻り値がその場で drop され、`ThreadGuard` の `Drop` が `CoUninitialize` を呼ぶため、Windows ではそのスレッドの COM 初期化カウンタが減る。他に `ThreadGuard` を保持していなければ、以降の COM 呼び出しは `CO_E_NOTINITIALIZED` になる。また、`initialize_thread` の doc には `let _guard = ...;` の例が無い。
- `src/runtime.rs` の `ThreadGuard` は `PhantomData<*const ()>` により既に `!Send` / `!Sync` だが、その性質を固定するコンパイルテストが無い。
- `src/capture.rs` / `src/playback.rs` の `AudioCaptureSession` / `AudioPlaybackSession` の doc は「ガードは別のスレッドへ move できる」と述べているが、move 先のスレッドで `initialize_thread()` を呼ぶ必要があることは書いていない。`AudioCaptureSession` / `AudioPlaybackSession` の `Drop` は `WasapiCaptureImpl::stop` / `WasapiPlaybackImpl::stop` を呼び、Windows では `IAudioClient::Stop` などの COM 呼び出しを行う。`tests/session_lifecycle.rs` の `stop_while_callback_blocked` は停止スレッドで `initialize_thread()` を呼んでいる。
- `src/runtime.rs` の `initialize_thread` の doc は「返された `ThreadGuard` はスレッドの終了まで保持すること」と述べているが、`AudioCapture` / `AudioPlayback` などの COM オブジェクトより先に drop してはいけないことを明示していない。型でも強制していない。

## 設計方針

- `src/runtime.rs` の `ThreadGuard` に `#[must_use = "..."]` を付け、`initialize_thread` の doc に `let _guard = initialize_thread()?;` のように名前付きで保持する例を追加する。`let _ = ...;` では即時 drop されるため、`_guard` という名前で保持する例にする。
- `initialize_thread` の doc に、返された `ThreadGuard` は同じスレッドで利用する `AudioCapture` / `AudioPlayback` などのオブジェクトをすべて drop した後に drop することを明記する。型で強制する API 変更は行わない。`AudioCapture` / `AudioPlayback` が `ThreadGuard` を借用すると、`ThreadGuard` が `!Sync` のため `Send` でなくなり、別スレッドへの move ができなくなるためである。
- `src/capture.rs` / `src/playback.rs` の `AudioCaptureSession` / `AudioPlaybackSession` の doc と README の「セッションの開始と停止」に、Windows で move 先のスレッドでも `initialize_thread()` を呼ぶ必要があることを明記する。
- `src/runtime.rs` の `ThreadGuard` の doc に `compile_fail` doctest を追加し、`Send` と `Sync` をそれぞれ要求する関数へ `ThreadGuard` を渡すとコンパイルエラーになることを固定する。既存の `AudioCapture::start` / `AudioPlayback::start` の `compile_fail` doctest と同じ方式にする。
- `Drop` から COM を呼ばない設計 (停止専用スレッドへの委譲など) は本 issue では採用しない。現状の Drop 停止を維持し、move 先スレッドの初期化要件を doc で明示する。

## 完了条件

- `initialize_thread()?;` のように結果を捨てると `unused_must_use` 警告が出る。
- `initialize_thread` の doc に `let _guard = ...;` で保持する例が追加されている。
- `initialize_thread` の doc に、`ThreadGuard` をオーディオオブジェクトより後に drop する要件が書かれている。
- move 先スレッドの初期化要件が `AudioCaptureSession` / `AudioPlaybackSession` の doc と README に書かれている。
- `ThreadGuard` が `!Send` / `!Sync` であることが `compile_fail` doctest で固定されている。
- `cargo test --workspace` が通る。
