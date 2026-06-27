# Linux に音声再生 (Playback) 機能を実装する

- Priority: Medium
- Created: 2026-06-27
- Completed: {YYYY-MM-DD}
- Model: Kimi Code CLI
- Branch: feature/add-linux-audio-playback
- Polished: {YYYY-MM-DD}

## 目的

Linux (PulseAudio / PipeWire) でも `AudioPlayback` を使って音声を再生できるようにする。
現在のライブラリは Linux でキャプチャは実装されているが、再生は `playback.rs` のスタブで `Error::SessionCreateFailed` を返すのみとなっている。

## 優先度根拠

Medium。Linux ユーザーへの機能対称性を提供するため。キャプチャに対して再生が欠けていると、双方向通信を想定したユースケースでプラットフォーム間の差異が生じる。

## 現状

- `src/playback.rs` (macOS/Linux 共用) は現在未実装のスタブ
- `src/audio_pulse.c` には `AudioSession` (キャプチャ) は存在するが、再生用の `PlaybackSession` API がない
- `src/audio_pipewire.c` には `AudioSession` (キャプチャ) は存在するが、再生用の `PlaybackSession` API がない
- `examples/playback_sine.rs` は Linux では実行できない

## 設計方針

- PulseAudio バックエンドでは `pa_stream_connect_playback` を使用する
- PipeWire バックエンドでは `PW_DIRECTION_OUTPUT` でストリームを接続する
- 両バックエンドとも S16 フォーマットで動作し、Rust 側で必要に応じて F32 からの変換を行う
- 既存のキャプチャ実装と対称的な `PlaybackSession` API を追加する

## 完了条件

- Linux 上で `AudioPlayback::new` / `start` / `stop` が PulseAudio / PipeWire 両方でエラーなく動作すること
- `examples/playback_sine.rs` がデフォルト出力デバイスから 440Hz サイン波を再生すること
- `cargo test --workspace` が Linux 上で両 feature (`pulse` / `pipewire`) とも全てパスすること

## 解決方法

- `src/audio_c.h` に `PlaybackSession` 用の FFI API (`playback_session_create` / `destroy` / `start` / `stop` / `sample_rate` / `channels`) を追加する
- `src/audio_pulse.c` に `pa_threaded_mainloop` + `pa_stream_connect_playback` による再生実装を追加する
- `src/audio_pipewire.c` に `pw_thread_loop` + `PW_DIRECTION_OUTPUT` による再生実装を追加する
- `src/playback.rs` のスタブを `ffi::PlaybackSession` 連携コードに置き換える
- `build.rs` の bindgen allowlist に playback 関連シンボルを追加する
