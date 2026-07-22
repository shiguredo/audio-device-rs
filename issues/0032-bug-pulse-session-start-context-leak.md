# audio_pulse.c の session_start 失敗後コンテキスト残留

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-pulse-session-start-context-leak
- Polished: 2026-07-21

## 目的

`src/audio_pulse.c` の `audio_pulse_session_start` / `audio_pulse_playback_session_start` で、`pa_context_connect` 成功後にストリーム生成や接続が失敗した場合にコンテキストが接続されたまま残留する問題を修正する。

## 優先度根拠

この状態で再度 `session_start` を呼ぶと `pa_context_connect` が `PA_ERR_BADSTATE` で失敗し、セッションが永久に使用不能になる。

## 現状

### キャプチャ側（audio_pulse_session_start: 470-557 行目）

`pa_context_connect` 成功後（492 行目）、コンテキストが READY 状態に到達した後のエラーパスで `pa_context_disconnect` が呼ばれていない:

1. ストリーム生成失敗（519-523 行目）:
```c
if (!session->stream) {
    pa_threaded_mainloop_unlock(session->mainloop);
    pa_threaded_mainloop_stop(session->mainloop);
    return -4;
    // pa_context_disconnect が呼ばれていない
}
```

2. `pa_stream_connect_record` 失敗（531-538 行目）
3. ストリーム状態待ちの FAILED/TERMINATED（541-548 行目）

なお、コンテキスト状態待ちの FAILED/TERMINATED パス（500-508 行目）はコンテキストが READY に到達していないため `pa_context_disconnect` 不要であり、修正対象外。

### 再生側（audio_pulse_playback_session_start: 753-841 行目）

キャプチャ側と同一のパターンで 3 箇所のエラーパスに `pa_context_disconnect` が欠落:

1. ストリーム生成失敗（802-806 行目）
2. `pa_stream_connect_playback` 失敗（815-822 行目）
3. ストリーム状態待ちの FAILED/TERMINATED（825-832 行目）

## 設計方針

各エラーパスで `pa_threaded_mainloop_unlock` の前に `pa_context_disconnect(session->context)` を追加する。PulseAudio の threaded mainloop の規約上、`pa_context_disconnect` はロック保持中に呼ぶ必要がある。

## 完了条件

- キャプチャ側 3 箇所 + 再生側 3 箇所 = 計 6 箇所のエラーパスで `pa_context_disconnect(session->context)` が呼ばれる
- `cargo clippy` / `cargo test` が通る（PulseAudio は Linux 専用のため、macOS 上のテストではこのコードパスは検証されない。ストリーム生成失敗・接続失敗の再現も CI 環境では不可能なため、コードレビューで担保する）

## 解決方法

キャプチャ側（519-523, 531-538, 541-548 行目）と再生側（802-806, 815-822, 825-832 行目）の各エラーパスで、`pa_threaded_mainloop_unlock` の前に `pa_context_disconnect(session->context);` を追加する。

## 後方互換

公開 API の変更なし。エラーパスでのリソース解放追加であり、正常パスの挙動は不変
