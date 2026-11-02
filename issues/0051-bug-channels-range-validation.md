# channels と sample_rate の範囲検証を追加する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-channels-range-validation
- Polished: {YYYY-MM-DD}

## 目的

公開 API から C バックエンドへ渡す `channels` と `sample_rate` の範囲を検証し、バックエンド側の値の切り詰めと符号付き整数オーバーフロー (未定義動作) を防ぐ。

## 現状

- `AudioCaptureConfig` と `AudioPlaybackConfig` の `channels` は i32 の任意値を受け付ける。doc は「0 以下の場合は既定値」としか書いておらず上限が無い。
- `FfiCaptureImpl::new` と `FfiPlaybackImpl::new` は `config.sample_rate` と `config.channels` を検証せず、そのまま `ops.session_create` に渡している。
- PulseAudio の `pa_sample_spec.channels` は uint8_t であり、`audio_pulse_session_create` の `.channels = channels` で切り詰められる。257〜287 などの値は 1〜31 に切り詰められ、`pa_sample_spec_valid` を通過して `pa_stream_new` と `pa_stream_connect_record` / `pa_stream_connect_playback` が成功する。一方 `session->channels` には元の値が残るため、`stream_read_callback` / `stream_write_callback` の `frame_size` 計算と実際のストリームのチャンネル数が食い違い、`AudioCapture::channels()` も誤った値を返す。
- 2^30 以上の `channels` では `audio_pulse.c` の `capture_timestamp_us` や各コールバックの `2 * session->channels` が int の範囲を超え、C の符号付き整数オーバーフロー (未定義動作) になる。PipeWire の `on_process` / `playback_on_process` にも同じ乗算がある。WASAPI は u16 変換と `build_wave_format` の `nBlockAlign` / `nAvgBytesPerSec` 検査で一部守られているが、Rust 層で共通の検証はしていない。
- `sample_rate` も Rust 層では上限が無く、各バックエンドの扱いが揃っていない。

## 設計方針

- Rust 側の共通ヘルパー (`common.rs` など) で `channels` と `sample_rate` の範囲を検証し、`Error::InvalidChannels` または `Error::InvalidAudioFormat` を返す。
- `channels` の共通上限は PulseAudio の `PA_CHANNELS_MAX` (32) を最小値とし、バックエンド固有の上限が必要な場合は各バックエンドで追加検証する。上限値は C 側とコメントで共有する。
- `sample_rate` は現実的な範囲 (例: 1〜768000) を共通で検証する。
- C 側でも `int` ではなく `size_t` / `int64_t` でフレームサイズを計算し、防御的に上限を検査する。
- `AudioCaptureConfig` / `AudioPlaybackConfig` の doc に許容範囲を明記する。

## 完了条件

- 上限外の `channels` / `sample_rate` を指定すると `Err` を返す。
- 257 ch 相当の値でストリームが作成されず、`AudioCapture::channels()` が実ストリームと異なる値を返さない。
- 極端な値でも C 側で符号付き整数オーバーフローが発生しない。
- 境界値と切り詰めケースの回帰テストが追加されている。
- `cargo fmt` / `cargo clippy` / `cargo test` が通る。
