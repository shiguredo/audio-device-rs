# determine_audio_format が 24/32 bit PCM を S16 と誤判定する

- Created: 2026-10-19
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-format-detection
- Polished: 2026-10-19

## 目的

Windows (WASAPI) の再生で、デバイスの mix format を S16 と誤判定し、24/32 bit PCM のデバイスでは壊れた音声を再生する問題を修正する。
mix format のサンプル形式を解釈せず、常に F32 で初期化してオーディオエンジンに変換させることで、任意の mix format のデバイスで正しく再生できるようにする。
壊れたデータが音声の再生や処理へ渡るのを防ぐ。

## 現状

`src/playback_wasapi.rs` の `WasapiPlaybackImpl::new` は `IAudioClient::GetMixFormat` の戻り値から `determine_audio_format` で S16 か F32 かを判定し、その結果に合わせてレンダーバッファへ書き込んでいる。
`determine_audio_format` は `WAVE_FORMAT_IEEE_FLOAT` と `WAVE_FORMAT_EXTENSIBLE` の float SubFormat だけを F32 と判定し、それ以外をすべて S16 と判定する。

`playback_thread_func` は `format.bytes_per_sample()` で `buffer_size` を計算する。
S16 と誤判定された PCM では、実際のレンダーバッファのうち `frames * channels * 2` バイトだけを `write_playback_frame_to_buffer` で書き、残りを書かないまま `ReleaseBuffer` で解放する。残りには古いデータが残り、再生音が壊れる。

- 24 bit コンテナ (`wBitsPerSample == 24`) では実際の 3 分の 2 のバイト列しか使わず、3 バイト単位のサンプル境界と 2 バイト単位の i16 境界がずれる
- 32 bit コンテナ (`wBitsPerSample == 32`、`wValidBitsPerSample == 24` を含む) では 2 分の 1 のバイト列だけを使う

Microsoft Learn の [`IAudioClient::GetMixFormat`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-getmixformat) の Remarks は
「the audio engine might use a mix format that represents samples as floating-point values」と述べるだけで、常に float32 を返す保証はしていない。
[`Device Formats`](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-formats) は「The method always uses a WAVEFORMATEXTENSIBLE structure」と述べている。
共有モードの mix format が常に float32 である保証はなく、24/32 bit PCM の `WAVEFORMATEXTENSIBLE` が返る可能性がある。

`AudioPlaybackConfig` の `sample_rate` と `channels` は無視されている。
capture は 0043 で S16 のストリームを要求するようになり、`determine_audio_format` の呼び出しはなくなっている。

## 設計方針

- 再生のストリーム形式を F32 固定にし、`AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM` と `AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY` を指定して、mix format への変換をオーディオエンジンに任せる。mix format のサンプル形式は解釈しない。
- `AudioPlaybackConfig` の `sample_rate` と `channels` を使う。0 以下の場合は 48000 Hz / 2 ch にする。
- `WAVEFORMATEX` の構築は録音と共通の `build_wave_format` にまとめる。
- 音声形式として表現できない値は `Error::InvalidAudioFormat` で返す。
- `determine_audio_format` は削除する。

## Windows 実機での確認手順

1. `cargo run --example device_info` で入力・出力デバイスを確認する
2. `AudioPlayback::new` と `AudioCapture::new` が成功することを確認する
3. コールバックが返すフレームを F32 と S16 のそれぞれで、`AudioPlayback` の start / stop が成功することを確認する
4. 44100 Hz を指定したときにエンジンがリサンプルし、0 以下の指定で 48000 Hz / 2 ch になることを確認する
5. 24/32 bit PCM のデバイスを用意できない場合、mix format のサンプル形式を解釈しないため影響しないことを確認する

## 完了条件

- `AudioPlayback::new` が mix format のサンプル形式に依存せず成功すること
- F32 と S16 のフレームで再生できること
- `AudioPlaybackConfig` の `sample_rate` と `channels` が反映されること
- `build_wave_format` の単体テストが S16 と F32 の各フィールド、およびフィールド幅を超える値を固定していること
- Windows で `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` がパスすること
- `CHANGES.md` の `## develop` に `[CHANGE]` と `[FIX]` エントリを追加すること
