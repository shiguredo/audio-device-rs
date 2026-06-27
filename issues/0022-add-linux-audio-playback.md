# Linux に音声再生 (Playback) 機能を実装する

- Priority: Medium
- Created: 2026-06-27
- Completed: {YYYY-MM-DD}
- Model: Kimi Code CLI
- Branch: feature/audio-output
- Polished: 2026-06-27

## 目的

Linux (PulseAudio / PipeWire) で `AudioPlayback` を使って音声を再生できるようにし、Windows / macOS と機能対称性を持たせる。

## 優先度根拠

Medium。Linux ユーザーへの機能対称性を提供するため。キャプチャに対して再生が欠けていると、双方向通信を想定したユースケースでプラットフォーム間の差異が生じる。

## 現状

`feature/audio-output` ブランチで Linux 再生機能が実装済み。
`develop` ブランチにはまだ統合されていない。

実装済みの主なファイルと役割:

- `src/audio_c.h` ... `PlaybackSession` 用の FFI API (`playback_session_create` / `destroy` / `start` / `stop` / `sample_rate` / `channels`) と `AudioPlaybackCallback` 型を宣言
- `src/audio_pulse.c` ... PulseAudio バックエンドの再生実装
- `src/audio_pipewire.c` ... PipeWire バックエンドの再生実装
- `src/playback.rs` ... `AudioPlayback` の Rust ラッパー。FFI 連携と S16 / F32 間のフォーマット変換を実装
- `build.rs` ... bindgen allowlist に `playback_.*` / `PlaybackSession` / `AudioPlaybackCallback` を追加
- `examples/playback_sine.rs` ... 440Hz サイン波再生サンプル

## 設計方針

- PulseAudio バックエンドでは `pa_threaded_mainloop` + `pa_stream_connect_playback` を使用する
- PipeWire バックエンドでは `pw_thread_loop` + `PW_DIRECTION_OUTPUT` でストリームを接続する
- 両バックエンドとも C 側は S16 フォーマット固定で動作し、Rust 側で `PlaybackFrame` のフォーマットに応じて F32 からの変換を行う
- 既存のキャプチャ実装と対称的な `PlaybackSession` API を追加する
- コールバック戻り値は `[0, frames]` にクランプし、不足分は無音で埋める

## 完了条件

- `feature/audio-output` ブランチが `develop` にマージされること
- Linux 上で `AudioPlayback::new` / `start` / `stop` が PulseAudio / PipeWire 両方でエラーなく動作すること
- `examples/playback_sine.rs` がデフォルト出力デバイスから 440Hz サイン波を再生すること
- `cargo test --workspace` が Linux 上で両 feature (`pulse` / `pipewire`) とも全てパスすること

## 解決方法

- `src/audio_c.h` に `PlaybackSession` 用の FFI API を追加する
- `src/audio_pulse.c` に以下を追加する:
  - `PlaybackSession` 構造体 (`pa_threaded_mainloop`、`pa_context`、`pa_stream`、コールバック、running フラグ)
  - `stream_write_callback` ... PulseAudio から呼ばれる書き込みコールバック。`pa_stream_begin_write` で取得したバッファに対して `AudioPlaybackCallback` を呼び出し、`pa_stream_write` で書き戻す
  - `playback_session_create` ... `pa_threaded_mainloop` / `pa_context` を作成
  - `playback_session_start` ... `pa_context_connect` 後、`pa_stream_new` + `pa_stream_connect_playback` で再生ストリームを接続
  - `playback_session_stop` ... `pa_stream_disconnect` / `pa_stream_unref` 後、`pa_threaded_mainloop_stop` で停止
  - `playback_session_destroy` ... ストリーム、コンテキスト、メインループを解放
  - `playback_session_sample_rate` / `playback_session_channels` ... 設定値を返す
- `src/audio_pipewire.c` に以下を追加する:
  - `PlaybackSession` 構造体 (`pw_thread_loop`、`pw_context`、`pw_core`、`pw_stream`、リスナ、コールバック、running フラグ)
  - `playback_on_process` ... PipeWire から呼ばれる process コールバック。`pw_stream_dequeue_buffer` で取得したバッファに対して `AudioPlaybackCallback` を呼び出し、`pw_stream_queue_buffer` で返却
  - `playback_session_create` ... `pw_thread_loop` / `pw_context` を作成
  - `playback_session_start` ... `pw_context_connect` 後、`pw_stream_new` + `pw_stream_connect` (`PW_DIRECTION_OUTPUT`) で再生ストリームを接続
  - `playback_session_stop` ... `pw_stream_disconnect` 後、`pw_thread_loop_stop` で停止
  - `playback_session_destroy` ... ストリーム、core、コンテキスト、スレッドループを解放
  - `playback_session_sample_rate` / `playback_session_channels` ... 設定値を返す
- `src/playback.rs` を FFI 連携コードに置き換え、`AudioFormat::from_ffi`、整数オーバーフロー防止、`catch_unwind`、unaligned 読み書きを含むフォーマット変換を実装する
- `build.rs` の bindgen allowlist に playback 関連シンボルを追加する
- `examples/playback_sine.rs` を追加する
