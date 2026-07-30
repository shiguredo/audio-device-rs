# audio_pipewire.c stop/start 再実行時の stream・core リークを修正する

- Created: 2026-07-30
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pipewire-stop-start-leak
- Polished: {YYYY-MM-DD}

## 目的

`audio_pipewire.c` のキャプチャ・再生両方で、`session_stop` 後に `session_start` を再実行すると古い stream と core がリークする問題を修正する。

## 現状

`audio_pipewire_session_stop`（および `audio_pipewire_playback_session_stop`）は stream を `pw_stream_disconnect` するだけで `pw_stream_destroy` せず、core も切断しない。`session->stream` と `session->core` にポインタが残ったままになる。

その後 `session_start` を再呼び出しすると:

1. `pw_context_connect` が新しい core を作成して `session->core` を上書きする（古い core がリーク）
2. `pw_stream_new` が新しい stream を作成して `session->stream` を上書きする（古い stream がリーク）

対照的に PulseAudio 側（`audio_pulse_session_stop`）は `pa_stream_disconnect` + `pa_stream_unref` + `session->stream = NULL` まで行っており、再実行時にリークしない。

キャプチャ・再生の両方に同じ問題がある。

## 設計方針

`session_stop` で stream の destroy と core の disconnect + hook 除去を行い、`NULL` を代入する。PulseAudio 側の `audio_pulse_session_stop` と同等のクリーンアップを行う。

## 完了条件

`session_stop` → `session_start` の再実行でリソースリークが発生しないこと。キャプチャ・再生の両方で修正されること。

## 解決方法

`src/audio_pipewire.c` の `audio_pipewire_session_stop` と `audio_pipewire_playback_session_stop` 関数内:

1. `pw_stream_disconnect` の後に `pw_stream_destroy(session->stream)` を呼び、`session->stream = NULL` を代入する
2. `spa_hook_remove(&session->core_listener)` を呼ぶ
3. `pw_core_disconnect(session->core)` を呼び、`session->core = NULL` を代入する

`session_start` 側でも、既存の stream/core が残っている場合に先に破棄する防御を追加する。
