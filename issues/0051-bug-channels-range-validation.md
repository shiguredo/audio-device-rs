# channels と sample_rate の範囲検証を追加する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-channels-range-validation
- Polished: 2026-11-03

## 目的

公開 API からバックエンドへ渡す `channels` と `sample_rate` の上限を検証し、PulseAudio の `pa_sample_spec.channels` の uint8_t 切り詰め、PulseAudio / PipeWire の C コードにおける符号付き整数オーバーフロー (未定義動作)、および上限超過によるセッション開始失敗を防ぐ。WASAPI を含む全バックエンドで同じ範囲を共通に扱う。

## 現状

- `AudioCaptureConfig` と `AudioPlaybackConfig` の `channels` は i32 の任意値を受け付ける。doc は「0 以下の場合は既定値」としか書いておらず上限が無い。
- `FfiCaptureImpl::new` と `FfiPlaybackImpl::new` は `config.sample_rate` と `config.channels` を検証せず、そのまま `ops.session_create` に渡している。`WasapiCaptureImpl::new` と `WasapiPlaybackImpl::new` は `u16` 変換と `build_wave_format` で一部を検査するが、Rust 層に共通の範囲検証は無い。
- PulseAudio の `pa_sample_spec.channels` は uint8_t であり、`create_stream` の `pa_sample_spec.channels = channels` で切り詰められる。`audio_pulse_session_create` は `session->channels` に元の値をそのまま保持する。257〜287 などの値は 1〜31 に切り詰められ、`pa_sample_spec_valid` を通過して `pa_stream_new` と `pa_stream_connect_record` / `pa_stream_connect_playback` が成功する。一方 `session->channels` には元の値が残るため、`stream_read_callback` / `stream_write_callback` の `frame_size` 計算と実際のストリームのチャンネル数が食い違い、`AudioCapture::channels()` / `AudioPlayback::channels()` も誤った値を返す。
- 2^30 以上の `channels` では、`audio_pulse.c` の `capture_timestamp_us` と各コールバックの `2 * session->channels`、および `audio_pipewire.c` の `on_process` / `playback_on_process` の `bytes_per_sample * session->channels` が int の範囲を超える。実際にストリームが作成される値では C の符号付き整数オーバーフロー (未定義動作) になる。WASAPI は `u16` 変換と `build_wave_format` の `nBlockAlign` / `nAvgBytesPerSec` 検査で一部守られている。
- `sample_rate` も共通の上限が無く、各バックエンドの扱いが揃っていない。PulseAudio 16.1 (CI の Ubuntu 24.04 が使うバージョン) の `PA_RATE_MAX` は 384000 であり、これを超える値は `pa_sample_rate_valid` の nominal 上限を超える。`pa_sample_rate_valid` は module-loopback 用に 1% の余裕を許すため 384001〜387840 は通過し得るが、共通上限は nominal な 384000 に合わせる。`pa_stream_new` が失敗した場合、セッション開始時に `Error::SessionStartFailed` になる。WASAPI も `WAVEFORMATEX` の `nAvgBytesPerSec` に収まらない値を拒否する。

## 設計方針

- `src/common.rs` に共通ヘルパーを追加し、`channels` と `sample_rate` の上限を検証する。上限超過時は `Error::InvalidAudioFormat` を返す。`Error::InvalidChannels` は `PlaybackFrame::from_s16` / `from_f32` の `channels <= 0` 用のまま変更しない。
- `channels` の共通上限は 32 (PulseAudio の `PA_CHANNELS_MAX`。バックエンドの上限のうち最小のもの) とする。`sample_rate` の共通上限は 384000 (PulseAudio 16.1 の `PA_RATE_MAX`。現行 master は 768000 だが、CI の Ubuntu 24.04 が使う 16.1 に合わせる) とする。
- `channels <= 0` と `sample_rate <= 0` は現行どおり既定値として扱う。キャプチャは 48000 Hz / 1 ch、再生は 48000 Hz / 2 ch とし、本 issue では既定値処理の所在を変更しない（既定値の集約は別 issue の範囲とする）。共通ヘルパーは 0 以下を許容し、上限だけを拒否する。
- 共通ヘルパーは `FfiCaptureImpl::new` / `FfiPlaybackImpl::new` / `WasapiCaptureImpl::new` / `WasapiPlaybackImpl::new` の先頭で、デバイス取得とセッション作成より前に呼ぶ。上限外の値はデバイスの有無にかかわらず `Error::InvalidAudioFormat` になる。
- `audio_pulse.c` の `capture_timestamp_us` / `stream_read_callback` / `stream_write_callback` と、`audio_pipewire.c` の `on_process` / `playback_on_process` では、フレームサイズを `int64_t` または `size_t` で計算し、`frame_size > 0` と `frames` が `int` に収まることを検査する。Rust 側で上限を検証した後でも、C 側で符号付き整数オーバーフローが起きないようにする。
- `audio.h` のセッション API のコメントに、`channels` の上限 32、`sample_rate` の上限 384000 を書く。0 以下の既定値は現行の記述を維持する。
- `AudioCaptureConfig` / `AudioPlaybackConfig` の doc に、0 以下の既定値、上限、上限超過時の `Error::InvalidAudioFormat` を明記する。`AudioCapture::new` / `AudioPlayback::new` の doc と README の該当箇所も同じ内容に揃える。
- `src/common.rs` の `#[cfg(test)]` に共通ヘルパーの境界値テストを追加する。0 と負値が許容されること、1 と 32 が許容されること、33 が拒否されること、1 と 384000 が許容されること、384001 と `i32::MAX` が拒否されることを固定する。
- `tests/test_capture.rs` / `tests/test_playback.rs` に、上限外の `channels` / `sample_rate` がデバイス取得前に `Err(Error::InvalidAudioFormat)` になることを検証するテストを追加する。有効側の境界値は共通ヘルパーの単体テストで固定する。

## 完了条件

- `channels > 32` または `sample_rate > 384000` を指定すると、デバイス取得前に `Err(Error::InvalidAudioFormat)` を返す。
- `channels <= 0` / `sample_rate <= 0` は範囲検証ではエラーにならず、現行どおりバックエンドの既定値 (キャプチャ 48000 Hz / 1 ch、再生 48000 Hz / 2 ch) として扱われる。デバイス取得やセッション開始に起因するエラーは別に返る。
- `channels = 257` は `Err(Error::InvalidAudioFormat)` となり、PulseAudio のストリームが作成されない。有効範囲の `channels` は 32 以下なので、PulseAudio の uint8_t 切り詰めによって `AudioCapture::channels()` / `AudioPlayback::channels()` が実ストリームと異なる値を返す経路がなくなる。
- `i32::MAX` などの上限側の極端な値は Rust 側で拒否され、C 側のフレームサイズ計算で符号付き整数オーバーフローが発生しない。
- 共通ヘルパーの境界値テストと、公開 API 経由の上限外テストが追加されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が、CI の全構成 (PulseAudio / PipeWire / CoreAudio / WASAPI) で通る。
