# PipeWire の stop() が stream / core を破棄しないため再起動でリーク・破綻する

- Priority: High
- Created: 2026-06-28
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-pipewire-stop-leak-restart
- Polished: {YYYY-MM-DD}

## 目的

`stop()` 後に `start()` を再び呼べるようにし、リソースリークと接続破綻を防ぐ。

## 優先度根拠

セッションのライフサイクルが「1 回だけ開始・停止」しか想定できない状態になっており、ライブラリとして再利用性が欠けている。連続した start/stop でリークが蓄積し、最終的に PipeWire 接続に失敗する。

## 現状

`src/audio_pipewire.c` の `audio_session_stop` / `playback_session_stop` は `pw_stream_disconnect` までしか行っていない。`pw_stream_destroy` および `pw_core_disconnect` は `audio_session_destroy` / `playback_session_destroy` でのみ実行されている。

その結果、ユーザーが `stop()` した後に再び `start()` すると、前回の `session->stream` と `session->core` が残ったまま `pw_context_connect` により新しい `core` が上書きされ、以下が発生する。

- 前回の `pw_stream` / `pw_core` が解放されずリークする
- `core_listener` の再登録が以前の状態と混在する可能性がある
- 連続した start/stop が失敗する

## 設計方針

`stop()` 時に `stream` と `core` を確実に破棄し、セッションを `start()` 可能な初期状態に戻す。`destroy()` 時には二重に破棄しないよう、ポインタを `NULL` に戻す。

## 完了条件

- `start()` -> `stop()` -> `start()` -> `stop()` を連続して成功すること
- 各種リソース（stream, core）が `stop()` 時に解放され、リーク検出ツールで検出されないこと
- 正常系の動作が変わらないこと

## 解決方法

`audio_session_stop` / `playback_session_stop` に以下を追加する。

1. `session->stream` が非 NULL なら `pw_stream_disconnect` の後に `pw_stream_destroy(session->stream)` を呼び、ポインタを `NULL` にする
2. `session->core` が非 NULL なら `spa_hook_remove(&session->core_listener)` の後に `pw_core_disconnect(session->core)` を呼び、ポインタを `NULL` にする
3. `start()` 側でも `session->stream` / `session->core` が残っていないか確認し、残っていたら破棄してから再接続する（防御的な実装）
