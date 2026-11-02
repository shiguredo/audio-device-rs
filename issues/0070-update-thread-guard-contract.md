# ThreadGuard の契約を must_use とドキュメントで守る

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-thread-guard-contract
- Polished: {YYYY-MM-DD}

## 目的

`ThreadGuard` の保持忘れと、ガードを別スレッドへ move した際の初期化要件の抜けを防ぐ。

## 現状

- `ThreadGuard` に `#[must_use]` が無いため、`initialize_thread()?;` と書くと戻り値がその場で drop され、Windows では COM が未初期化のまま残る。
- `AudioCaptureSession` / `AudioPlaybackSession` の doc は「ガードは別のスレッドへ move できる」とだけ述べ、move 先のスレッドでも `initialize_thread()` が必要であることを書いていない。Drop 経由の停止は Windows で `IAudioClient::Stop` などの COM 呼び出しを行う。`tests/session_lifecycle.rs` は実際に停止スレッドで `initialize_thread()` を呼んでいる。
- `ThreadGuard` を `AudioCapture` / `AudioPlayback` より先に drop した場合の COM 解放順序も型で強制されていない。

## 設計方針

- `ThreadGuard` に `#[must_use = "..."]` を付け、`initialize_thread` の doc に `let _guard = ...;` の正しい使い方の例を追加する。
- 両セッション型の doc と README に、Windows で move 先のスレッドでも `initialize_thread()` を呼ぶ必要があることを明記する。
- Drop から COM を呼ばない設計 (停止専用スレッドへの委譲など) を検討する。
- `ThreadGuard` が `!Send` / `!Sync` であることをコンパイルテストで固定する。

## 完了条件

- `initialize_thread()?;` のように結果を捨てると警告が出る。
- move 先スレッドの初期化要件が doc と README に書かれている。
- `ThreadGuard` の `!Send` / `!Sync` がテストで固定されている。
- `cargo test --workspace` が通る。
