# PulseAudio の stop() が context を切断しないため再起動に失敗する

- Priority: High
- Created: 2026-06-28
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-pulseaudio-stop-context-reconnect
- Polished: {YYYY-MM-DD}

## 目的

PulseAudio セッションの `stop()` 後に `start()` を再び成功させる。

## 優先度根拠

`AudioPlayback` / `AudioCapture` の使用者が `stop()` 後に `start()` すると、現状は再接続に失敗する。ライブラリとしての基本的なライフサイクルが成立していない。

## 現状

`src/audio_pulse.c` の `audio_session_stop` / `playback_session_stop` は `pa_stream_disconnect` / `pa_stream_unref` までしか行っていない。`pa_context_disconnect` は `audio_session_destroy` / `playback_session_destroy` でのみ実行されている。

`pa_context_connect` は `pa_context_get_state(c) == PA_CONTEXT_UNCONNECTED` の場合にのみ成功する。`stop()` 後の context は `PA_CONTEXT_READY` などの状態のままであるため、2 回目の `start()` で `pa_context_connect` が失敗する。

## 設計方針

`stop()` 時に `pa_context_disconnect` を呼び、context を未接続状態に戻す。ただし `pa_context_disconnect` は `PA_CONTEXT_TERMINATED` に遷移させるため、`start()` 側では既存の context を unref して新規に作成するか、再接続可能な状態に整える必要がある。

## 完了条件

- `start()` -> `stop()` -> `start()` -> `stop()` を連続して成功すること
- 正常系の動作が変わらないこと
- PulseAudio サーバー切断後の復帰も考慮された実装であること

## 解決方法

`audio_session_stop` / `playback_session_stop` において、`pa_stream_disconnect` / `pa_stream_unref` の後に `pa_context_disconnect(session->context)` を呼び出す。

さらに `start()` 側では、`pa_context_connect` 前に context の状態を確認し、`PA_CONTEXT_UNCONNECTED` でない場合は `pa_context_disconnect` してから再接続するか、新しい `pa_context` を作成する。
