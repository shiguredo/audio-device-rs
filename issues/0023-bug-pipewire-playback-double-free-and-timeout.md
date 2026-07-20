# audio_pipewire.c の pw_properties 二重解放とタイムアウト追加

- Priority: High
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-pipewire-playback-double-free-and-timeout
- Polished: {YYYY-MM-DD}

## 目的

`src/audio_pipewire.c` に存在する 2 つのバグを修正する。

1. 再生セッション開始失敗時の `pw_properties` 二重解放
2. ストリーム状態待ちループにタイムアウトがなくハングする可能性

## 優先度根拠

二重解放はヒープ破壊を引き起こす経路。タイムアウトなしは PipeWire デーモンが無応答に陥った場合に呼び出しスレッドが永久にブロックする。

## 現状

### 二重解放（src/audio_pipewire.c:858-860）

```c
session->stream =
    pw_stream_new(session->core, "audio-playback", props);
if (!session->stream) {
    pw_properties_free(props); // 二重解放
```

`pw_stream_new` は成否に関わらず `props` の所有権を取得する（PipeWire API 仕様）。キャプチャ側 (`audio_pipewire_session_start`) では同じ失敗パスで `pw_properties_free` を呼んでおらず、こちらが正しい。

### タイムアウトなし（src/audio_pipewire.c:530-555, 905-930）

```c
while (1) {
    enum pw_stream_state state = pw_stream_get_state(session->stream, NULL);
    if (state == PW_STREAM_STATE_STREAMING) break;
    // ...
    pw_thread_loop_wait(session->thread_loop); // 無限待機
}
```

## 完了条件

- 再生セッション開始失敗時のエラーパスから `pw_properties_free(props)` が削除される
- ストリーム状態待ちにタイムアウト（例: 10 秒）が設定される
- キャプチャ・再生の両方でタイムアウトが設定される

## 解決方法

1. `audio_pipewire_playback_session_start` のエラーパスから `pw_properties_free(props)` を削除する
2. `pw_thread_loop_wait` を `pw_thread_loop_timed_wait` に置き換え、タイムアウト（例: 10 秒）を設定する
3. タイムアウト後はエラーとして return する
