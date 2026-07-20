# audio_pulse.c の session_start 失敗後コンテキスト残留

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-pulse-session-start-context-leak
- Polished: {YYYY-MM-DD}

## 目的

`src/audio_pulse.c` の `audio_pulse_session_start` で、`pa_context_connect` 成功後にストリーム生成や接続が失敗した場合にコンテキストが接続されたまま残留する問題を修正する。

## 優先度根拠

この状態で再度 `session_start` を呼ぶと `pa_context_connect` が `PA_ERR_BADSTATE` で失敗し、セッションが永久に使用不能になる。

## 現状

src/audio_pulse.c:519-548 のエラーパス:

```c
// ストリーム生成失敗時
if (!session->stream) {
    pa_threaded_mainloop_unlock(session->mainloop);
    pa_threaded_mainloop_stop(session->mainloop);
    return -4;
    // pa_context_disconnect が呼ばれていない
}
```

同様のパターンが `pa_stream_connect_record` 失敗時、ストリーム状態待ちの FAILED/TERMINATED 時にも存在する。再生側 (`audio_pulse_playback_session_start`) も同様。

## 完了条件

- 各エラーパスで `pa_context_disconnect(session->context)` が呼ばれる
- キャプチャ・再生の両方で修正される
- `cargo clippy` / `cargo test` が通る

## 解決方法

各エラーパスで `pa_threaded_mainloop_unlock` の前に `pa_context_disconnect(session->context)` を追加する。
