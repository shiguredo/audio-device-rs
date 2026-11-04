# 音声設定の既定値と変換の契約を統一する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-audio-defaults-and-conversions
- Polished: 2026-11-04

## 目的

`sample_rate` / `channels` の既定値が Rust / C / README に散在している状態と、
フォーマット変換のスケール・丸め・endian・境界の契約が不明な状態を整理する。
既定値の実装を Rust 側の 1 箇所に集約し、C 側は確定値を受け取る契約に変更する。
あわせて、変換の契約を doc とテストで固定し、サポート対象のアーキテクチャと
endian を明記する。

## 現状

- 次の箇所で、`sample_rate` が 0 以下なら 48000、`channels` が 0 以下なら
  キャプチャ 1 / 再生 2 という同じ契約が独立に実装・記述されている。
  片方だけ変更しても他が追随しない。
  - `src/audio.h` のセッション API 共通仕様の create のコメント
  - `src/common.rs` の `AudioCaptureConfig` / `AudioPlaybackConfig` の
    フィールド doc と `Default` 実装
  - `src/capture.rs` の `AudioCapture::new` の doc
  - `README.md` のキャプチャ節
  - `src/audio_pulse.c` の `audio_pulse_session_create` /
    `audio_pulse_playback_session_create`
  - `src/audio_pipewire.c` の `audio_pipewire_session_create` /
    `audio_pipewire_playback_session_create`
  - `src/audio_coreaudio.m` の `audio_coreaudio_session_create` /
    `audio_coreaudio_playback_session_create`
  - `src/capture_wasapi.rs` の `WasapiCaptureImpl::new` /
    `src/playback_wasapi.rs` の `WasapiPlaybackImpl::new`
- `src/common.rs` の `write_playback_frame_to_buffer` の F32 → S16 は
  `(clamped * 32767.0) as i16`、S16 → F32 は `sample as f32 / 32768.0` で、
  スケールが非対称である。F32 の `-1.0` は S16 の `-32767` になり、
  S16 の `-32768` を F32 に変換して戻すと `-32767` になって最下位ビットが変わる。
  変換式と丸め方法が doc に無い。
- `src/common.rs` の `PlaybackFrame::from_s16` / `from_f32` は `to_le_bytes` で
  格納するが、`write_playback_frame_to_buffer` は `*const f32` / `*const i16` を
  `read_unaligned` で native-endian として読む。`AudioFrame::as_s16` / `as_f32` も
  native-endian として読む。C バックエンドは PulseAudio / PipeWire で
  `PA_SAMPLE_S16LE` / `SPA_AUDIO_FORMAT_S16_LE` を要求する。`data` の表現契約と
  サポート対象の endian が doc に無く、big-endian では無警告でバイト順が壊れる。
- 32 bit ターゲットでは、`write_playback_frame_to_buffer` の
  `dst_channels * bytes_per_sample` が `usize` でオーバーフローし得る。
  release ビルドでは wrap して `copy_len / sample_size` がゼロ除算になり得る、
  debug ビルドでは乗算オーバーフローで panic する。PulseAudio / PipeWire の
  C 側でも `frame_size = bytes_per_sample * session->channels` が `int` で
  オーバーフローし得る。サポート方針が書かれていない。
- C 側の `int frames = (int)(length / frame_size)`（PulseAudio / PipeWire）や
  `int frames = (int)(buffer->mAudioDataByteSize / bytes_per_frame)`（CoreAudio）は、
  商が `int` の範囲を超えると切り詰める。実際のパケット長では到達しにくいが、
  境界の契約が無い。
- デバイス列挙の `channels` / `sample_rate` の既定値（CoreAudio / PulseAudio /
  PipeWire / WASAPI のデバイス情報）は issue 0082 の範囲であり、
  本 issue では扱わない。

## 関連 issue

- `issues/0072-doc-public-api-contracts.md`: 公開 API の rustdoc の契約
  （`PlaybackFrame.data` の endian、`AudioPlayback::new` の既定値など）は
  0072 を正本とし、本 issue では重複して詳細を書かない。
- `issues/0073-doc-repository-documentation-consistency.md`: README / docs /
  `CHANGES.md` 全体の整合は 0073 の範囲とし、本 issue では既定値と変換の契約に
  限定する。
- `issues/0082-update-device-info-contract.md`: デバイス情報の意味と既定値は
  0082 の範囲。
- `issues/0083-change-playback-frame-private-fields.md`: `PlaybackFrame` の
  フィールド非公開化とアクセサ追加は 0083 の範囲。本 issue の endian の doc と
  競合しない。

## 設計方針

- 既定値の正本を `src/common.rs` の 1 箇所に置く。`DEFAULT_SAMPLE_RATE` /
  `DEFAULT_CAPTURE_CHANNELS` / `DEFAULT_PLAYBACK_CHANNELS` の定数と、
  `sample_rate` / `channels` が 0 以下の場合に既定値を返す `pub(crate)` 関数を
  定義する。`AudioCaptureConfig::default` / `AudioPlaybackConfig::default` と、
  `FfiCaptureImpl::new` / `FfiPlaybackImpl::new`、
  `WasapiCaptureImpl::new` / `WasapiPlaybackImpl::new` はこの定数・関数を使う。
  C 側の `*_session_create` から既定値処理を削除し、`src/audio.h` の create の
  契約を「呼び出し側が 1 以上の確定値を渡す」に変更する。C 側で 0 以下を
  受け取った場合は `NULL` を返す。
- 変換のスケールは、既存の変換結果とテストを維持するため、現行の
  `(clamped * 32767.0) as i16` / `sample as f32 / 32768.0` を正本として doc に
  明記する。丸めは `as i16` の 0 方向への切り捨て（truncation）であること、
  `NaN` は 0、`±Inf` は clamp 後に ±32767 になることを書く。スケール自体の
  変更は本 issue の範囲外とする。
- `PlaybackFrame::data` と `AudioFrame::data` は little-endian の PCM データと
  する。`write_playback_frame_to_buffer` は native-endian で読み書きするが、
  サポート対象を little-endian に限定することで契約と実装を一致させる。
  `to_ne_bytes` には変更しない（C バックエンドが `PA_SAMPLE_S16LE` /
  `SPA_AUDIO_FORMAT_S16_LE` を要求しているため）。サポート対象は 32-bit /
  64-bit の little-endian とし、big-endian はサポート外とする。big-endian では
  `#[cfg(target_endian = "big")] compile_error!` でビルドを失敗させ、
  無警告の破壊を防ぐ。README の対応プラットフォームに、サポート対象の
  アーキテクチャと endian を明記する。
- 32-bit もサポート対象とする。`write_playback_frame_to_buffer` の
  `dst_channels * bytes_per_sample` は `checked_mul` で計算し、オーバーフロー時は
  何も書き込まずに 0 を返す。C 側の `frame_size` が `int` でオーバーフローしない
  よう、`channels` の上限を `INT_MAX / 2`（S16 の `bytes_per_sample` は 2）と
  する。上限を超える場合は `Error::InvalidAudioFormat` を返す。`src/audio.h` の
  create の契約に `channels` の上限を書き、Rust 側の既定値解決関数でも上限を
  検証する。
- C 側の `frames` は `int` の範囲を超えない契約とする。`length / frame_size` が
  `INT_MAX` を超える場合は `INT_MAX` にクランプし、超過分は破棄する。
  `frame_size` は `channels` の上限により `int` に収まる。`src/audio.h` の
  コールバックの契約に、`frames` が `INT_MAX` 以下であることと、超過分が
  破棄されることを書く。
- `src/common.rs` の `write_playback_frame_to_buffer` の doc に、変換式、丸め、
  `NaN` / `±Inf` の扱い、`dst_channels` が 0 の場合と `sample_size` が
  オーバーフローした場合の戻り値を書く。`PlaybackFrame::from_s16` / `from_f32` の
  doc に、`data` が little-endian であることと、変換スケール・丸めを書く
  （0072 で既に書かれている場合はそれを正本とする）。`AudioFrame` の doc に、
  `data` が little-endian であることを書く。`AudioCaptureConfig` /
  `AudioPlaybackConfig` の doc は、既定値の数値を繰り返さず
  `AudioCaptureConfig::default()` / `AudioPlaybackConfig::default()` を参照する。
  README のキャプチャ・再生の節は rustdoc を正本とし、既定値の数値を
  繰り返さない（0072 / 0073 の方針に合わせる）。
- 公開動作が変わる変更（32-bit のオーバーフロー修正など）を行う場合は、
  `CHANGES.md` の `## develop` に追記する。

## 完了条件

- 既定値の実装が `src/common.rs` の 1 箇所に集約され、
  `AudioCaptureConfig::default` / `AudioPlaybackConfig::default`、
  `FfiCaptureImpl::new` / `FfiPlaybackImpl::new`、
  `WasapiCaptureImpl::new` / `WasapiPlaybackImpl::new` が同じ定数・関数を
  使っている。C 側の `*_session_create` から既定値処理が削除され、
  `src/audio.h` の create の契約が「呼び出し側が 1 以上の確定値を渡す」に
  変更され、C 側で 0 以下を受け取った場合は `NULL` を返す。
- `src/common.rs` の `write_playback_frame_to_buffer`、
  `PlaybackFrame::from_s16` / `from_f32`、`AudioFrame` の doc に、変換式、丸め、
  `NaN` / `±Inf` の扱い、`data` の little-endian 契約、`dst_channels` が 0
  または `sample_size` がオーバーフローした場合の戻り値が書かれている。
- `AudioCaptureConfig` / `AudioPlaybackConfig` の doc と README のキャプチャ・
  再生の節が、既定値の数値を繰り返さず、`AudioCaptureConfig::default()` /
  `AudioPlaybackConfig::default()` または rustdoc を参照している。
- README の対応プラットフォームに、サポート対象のアーキテクチャと endian
  （32-bit / 64-bit の little-endian、big-endian はサポート外）が書かれている。
  big-endian では `compile_error!` でビルドが失敗する。
- `src/audio.h` の create / コールバックの契約に、`channels` の上限、
  `frames` が `INT_MAX` 以下であること、`length / frame_size` が `INT_MAX` を
  超える場合のクランプと超過分の破棄が書かれている。
- `src/common.rs` の `#[cfg(test)]` に、F32 → S16 → F32 と S16 → F32 → S16 の
  往復変換テストが追加され、`-1.0` / `-32768` / `+1.0` / `32767` の契約値と、
  `dst_channels` が大きく `sample_size` がオーバーフローする場合の戻り値が
  検証されている。
- 既定値の解決関数の単体テストが追加され、0 以下と正値の扱い、`channels` の
  上限超過時の `Error::InvalidAudioFormat` が固定されている。
- `cargo fmt --all -- --check` /
  `cargo clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace` が、CI の全構成（PulseAudio / PipeWire / CoreAudio /
  WASAPI）で通る。
- 公開動作が変わる変更を行う場合は、`CHANGES.md` の `## develop` に
  追記されている。
