# macOS に音声再生 (Playback) 機能を実装する

- Priority: Medium
- Created: 2026-06-27
- Completed: {YYYY-MM-DD}
- Model: Kimi Code CLI
- Branch: feature/audio-output
- Polished: 2026-06-27

## 目的

macOS で `AudioPlayback` を使って音声を再生できるようにし、Windows / Linux と機能対称性を持たせる。

## 優先度根拠

Medium。macOS ユーザーへの機能対称性を提供するため。キャプチャに対して再生が欠けていると、双方向通信を想定したユースケースでプラットフォーム間の差異が生じる。

## 現状

`feature/audio-output` ブランチで macOS 再生機能が実装済み。
`develop` ブランチにはまだ統合されていない。

実装済みの主なファイルと役割:

- `src/audio_c.h` ... `PlaybackSession` 用の FFI API (`playback_session_create` / `destroy` / `start` / `stop` / `sample_rate` / `channels`) と `AudioPlaybackCallback` 型を宣言
- `src/audio_c.m` ... `AudioQueueNewOutput` + `AudioQueueEnqueueBuffer` を使った macOS ネイティブ再生実装
- `src/playback.rs` ... `AudioPlayback` の Rust ラッパー。FFI 連携と S16 / F32 間のフォーマット変換を実装
- `build.rs` ... bindgen allowlist に `playback_.*` / `PlaybackSession` / `AudioPlaybackCallback` を追加
- `examples/playback_sine.rs` ... 440Hz サイン波再生サンプル

## 設計方針

- macOS ネイティブ層は `AudioQueueNewOutput` + `AudioQueueEnqueueBuffer` を使用する
- キャプチャ実装 (`AudioSession`) と対称的な `PlaybackSession` API を追加する
- トリプルバッファリング（10ms x 3）で再生遅延を抑える
- C 側は S16 フォーマット固定で動作し、Rust 側で `PlaybackFrame` のフォーマットに応じて F32 からの変換を行う
- デバイス指定は `kAudioQueueProperty_CurrentDevice` で行う。指定デバイスが見つからない場合やプロパティ設定に失敗した場合はエラーにする
- コールバック戻り値は `[0, frames]` にクランプし、不足分は無音で埋める

## 完了条件

- `feature/audio-output` ブランチが `develop` にマージされること
- macOS 上で `AudioPlayback::new` / `start` / `stop` がエラーなく動作すること
- `examples/playback_sine.rs` がデフォルト出力デバイスから 440Hz サイン波を再生すること
- `cargo test --workspace` が macOS 上で全てパスすること

## 解決方法

- `src/audio_c.h` に `PlaybackSession` 用の FFI API を追加する
- `src/audio_c.m` に以下を追加する:
  - `PlaybackSession` 構造体 (`AudioQueueRef`、3 つの `AudioQueueBufferRef`、`AudioStreamBasicDescription`、コールバック、running フラグ)
  - `audio_output_callback` ... AudioQueue から呼ばれるコールバック。C バッファに対して Rust からの `AudioPlaybackCallback` を呼び出し、無音埋めを行う
  - `playback_session_create` ... `AudioQueueNewOutput` で出力キューを作成し、10ms x 3 のバッファを確保
  - `playback_session_start` ... 無音でバッファを初期化してエンキューし、`AudioQueueStart` で再生開始
  - `playback_session_stop` ... `AudioQueueStop` で停止
  - `playback_session_destroy` ... `AudioQueueDispose` で解放
  - `playback_session_sample_rate` / `playback_session_channels` ... 実際の値を返す
- `src/playback.rs` を FFI 連携コードに置き換え、`AudioFormat::from_ffi`、整数オーバーフロー防止、`catch_unwind`、unaligned 読み書きを含むフォーマット変換を実装する
- `build.rs` の bindgen allowlist に playback 関連シンボルを追加する
- `examples/playback_sine.rs` を追加する
