# PipeWire 再生の pw_stream_new 失敗時に props を二重解放する

- Priority: High
- Created: 2026-06-28
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-pipewire-playback-props-double-free
- Polished: {YYYY-MM-DD}

## 目的

PipeWire 再生セッション作成失敗時の二重解放をなくし、メモリ破壊やクラッシュを防ぐ。

## 優先度根拠

`pw_properties_free` の二重解放は未定義動作であり、即座のクラッシュやヒープ破壊につながる。正常系では発生しないが、リソース不足・PipeWire 接続失敗など異常系で顕在化する。

## 現状

`src/audio_pipewire.c` の `playback_session_start` において、`pw_stream_new(session->core, "audio-playback", props)` が失敗した場合に `pw_properties_free(props)` を呼び出している。

PipeWire の `pw_stream_new` は `props` の所有権を取得し、失敗時には内部で `pw_properties_free(props)` を実行する。したがって呼び出し元が再度 `pw_properties_free(props)` すると二重解放になる。

同ファイルのキャプチャ側 `audio_session_start` では失敗時に `pw_properties_free(props)` を呼んでおらず、正しい。

## 設計方針

`pw_stream_new` 失敗時には `props` を解放しない。キャプチャ側と同じ記述に統一する。

## 完了条件

- `pw_stream_new` 失敗時に `props` が二重解放されないこと
- 正常系の動作が変わらないこと
- 既存の PipeWire 再生テストまたは手動検証が通ること

## 解決方法

`src/audio_pipewire.c` の `playback_session_start` 内、`if (!session->stream)` ブロックから `pw_properties_free(props);` の呼び出しを削除する。
