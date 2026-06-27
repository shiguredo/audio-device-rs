# macOS に音声再生 (Playback) 機能を実装する

- Priority: Medium
- Created: 2026-06-27
- Completed: {YYYY-MM-DD}
- Model: Kimi Code CLI
- Branch: feature/add-macos-audio-playback
- Polished: {YYYY-MM-DD}

## 目的

macOS でも Windows と同様に `AudioPlayback` を使って音声を再生できるようにする。
現在のライブラリは macOS でキャプチャは実装されているが、再生は `playback.rs` のスタブで `Error::SessionCreateFailed` を返すのみとなっている。

## 優先度根拠

Medium。macOS ユーザーへの機能対称性を提供するため。キャプチャに対して再生が欠けていると、双方向通信を想定したユースケースでプラットフォーム間の差異が生じる。

## 現状

- `src/playback.rs` (macOS/Linux 共用) は現在未実装のスタブ
- `src/audio_c.m` には `AudioSession` (キャプチャ) は存在するが、再生用の `PlaybackSession` API がない
- `examples/playback_sine.rs` は macOS では実行できない

## 設計方針

- macOS ネイティブ層は `AudioQueueNewOutput` + `AudioQueueEnqueueBuffer` を使用する
- キャプチャ実装 (`audio_input_callback` / `AudioSession`) と対称的な構造で `PlaybackSession` API を追加する
- S16 フォーマットで動作し、Rust 側で必要に応じて F32 からの変換を行う
- デバイス指定は `kAudioQueueProperty_CurrentDevice` で行う

## 完了条件

- macOS 上で `AudioPlayback::new` / `start` / `stop` がエラーなく動作すること
- `examples/playback_sine.rs` がデフォルト出力デバイスから 440Hz サイン波を再生すること
- `cargo test --workspace` が macOS 上で全てパスすること

## 解決方法

- `src/audio_c.h` に `PlaybackSession` 用の FFI API (`playback_session_create` / `destroy` / `start` / `stop` / `sample_rate` / `channels`) を追加する
- `src/audio_c.m` に `AudioQueue` 出力実装 (`PlaybackSession`、`audio_output_callback`) を追加する
- `src/playback.rs` のスタブを `ffi::PlaybackSession` 連携コードに置き換える
- `build.rs` の bindgen allowlist に playback 関連シンボルを追加する
