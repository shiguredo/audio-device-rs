# audio_pipewire.c キャプチャ側 session_start エラーパスのリソースリークを修正する

- Created: 2026-07-30
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pipewire-capture-error-path-leak
- Polished: {YYYY-MM-DD}

## 目的

`audio_pipewire.c` のキャプチャ側 `audio_pipewire_session_start` のエラーパスで、`pw_properties` の未解放と `spa_hook_remove` の欠落によるリソースリークを修正する。

## 現状

再生側 `audio_pipewire_playback_session_start` のエラーパスでは `pw_properties_free` と `spa_hook_remove` が正しく呼ばれているが、キャプチャ側 `audio_pipewire_session_start` の対応するエラーパスでは両方が欠落している。

具体的には 2 箇所のエラーパスに問題がある:

1. `pw_properties_new` が失敗した場合: `spa_hook_remove(&session->core_listener)` が呼ばれていない（再生側にはある）
2. `pw_stream_new` が失敗した場合: `pw_properties_free(props)` と `spa_hook_remove(&session->core_listener)` の両方が呼ばれていない（再生側には両方ある）

再生側の対応箇所:

- `pw_properties_new` 失敗時: `spa_hook_remove` → `pw_core_disconnect` の順で正しく処理している
- `pw_stream_new` 失敗時: `pw_properties_free` → `spa_hook_remove` → `pw_core_disconnect` の順で正しく処理している

## 設計方針

再生側 `audio_pipewire_playback_session_start` のエラーパスと同一の処理をキャプチャ側に追加する。

## 完了条件

キャプチャ側 `audio_pipewire_session_start` の全エラーパスで、再生側と同等のリソース解放が行われること。

## 解決方法

`src/audio_pipewire.c` の `audio_pipewire_session_start` 関数内:

1. `pw_properties_new` 失敗時のエラーパスに `spa_hook_remove(&session->core_listener)` を追加する
2. `pw_stream_new` 失敗時のエラーパスに `pw_properties_free(props)` と `spa_hook_remove(&session->core_listener)` を追加する
