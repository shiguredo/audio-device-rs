# determine_audio_format が 24/32 bit PCM を S16 と誤判定する

- Created: 2026-10-19
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-format-detection
- Polished: 2026-10-19

## 目的

Windows (WASAPI) の `determine_audio_format` が `WAVE_FORMAT_EXTENSIBLE` の非 float (PCM の 24/32 bit) を S16 と誤判定する問題を修正する。
あわせて、8 bit PCM や 64 bit float など他の未対応形式も S16 や F32 と誤判定しないようにする。
誤判定すると、キャプチャでは `AudioFrame.data` が実際のバッファより短くなり、再生ではレンダーバッファの一部しか書かれない。どちらも波形が壊れる。
壊れたデータが音声の再生や処理へ渡るのを防ぐ。

## 現状

`src/device_wasapi.rs` の `determine_audio_format` は、`WAVE_FORMAT_IEEE_FLOAT` と `WAVE_FORMAT_EXTENSIBLE` の float SubFormat だけを F32 と判定し、それ以外をすべて S16 と判定する。
`AudioFormat` は S16 と F32 の 2 値しか持たない。

`src/capture_wasapi.rs` の `capture_thread_func` は `frames * channels * bytes_per_sample` で `data_size` を計算する。
S16 と誤判定された PCM では、実際のバッファのうち `frames * channels * 2` バイトだけを切り出して i16 として解釈する。

- 24 bit コンテナ (`wBitsPerSample == 24`) では実際の 3 分の 2 のバイト列しか使わず、3 バイト単位のサンプル境界と 2 バイト単位の i16 境界がずれる
- 32 bit コンテナ (`wBitsPerSample == 32`、`wValidBitsPerSample == 24` を含む) では 2 分の 1 のバイト列だけを使う

`AudioFrame::as_s16` は `data.len() >= frames * channels * 2` を満たすため、この不整合はエラーにならずにすり抜ける。

`src/playback_wasapi.rs` の `playback_thread_func` も `format.bytes_per_sample()` で `buffer_size` を計算する。
S16 と誤判定された PCM では、実際のレンダーバッファのうち `frames * channels * 2` バイトだけを `write_playback_frame_to_buffer` で書き、残りを書かないまま `ReleaseBuffer` で解放する。残りには古いデータが残り、再生音が壊れる。

Microsoft Learn の [`IAudioClient::GetMixFormat`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-getmixformat) の Remarks は
「the audio engine might use a mix format that represents samples as floating-point values」と述べるだけで、常に float32 を返す保証はしていない。
[`Device Formats`](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-formats) は「The method always uses a WAVEFORMATEXTENSIBLE structure」と述べている。
共有モードの mix format が常に float32 である保証はない。24/32 bit PCM の `WAVEFORMATEXTENSIBLE` が返る可能性がある。
24/32 bit PCM など S16 として扱えない float 以外の形式が返る環境では壊れた音声になる。

0043 は capture の要求フォーマットを S16 に固定し、`determine_audio_format(mix_format)` を capture から削除する。
0043 を先に実装する場合、本 issue の capture 側の呼び出し修正は不要になる。本 issue は `determine_audio_format` 自体と playback 側の呼び出しを対象とする。

## 設計方針

- `determine_audio_format` を `Result<AudioFormat>` を返す形に変更し、未対応形式を `Error::UnsupportedFormat(u16, u16)` (形式タグと bit 数) で明示的に返す。
  `Error` は公開 enum なので variant 追加は後方互換でない変更であり、`Display` に形式タグと bit 数を出す。
  `AudioFormat` への 24/32 bit PCM の追加は、公開 API (`AudioFormat`、`as_s16`、`as_f32`) とクレート内の `bytes_per_sample`、全バックエンドの変換処理に波及するため本 issue では行わない。
- `WAVE_FORMAT_EXTENSIBLE` の場合は [`WAVEFORMATEXTENSIBLE`](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible) の要件どおり `cbSize >= 22` を検証する。
  22 未満は `WAVEFORMATEXTENSIBLE` の `SubFormat` を読めない不正な形式として `Error::UnsupportedFormat` にする。
- `WAVE_FORMAT_EXTENSIBLE` の `SubFormat` が `KSDATAFORMAT_SUBTYPE_IEEE_FLOAT` の場合は `wBitsPerSample == 32` を F32、それ以外は `Error::UnsupportedFormat` にする。
- `WAVE_FORMAT_EXTENSIBLE` の `SubFormat` が `KSDATAFORMAT_SUBTYPE_PCM` の場合は `wBitsPerSample == 16` かつ `wValidBitsPerSample <= 16` を S16、それ以外は `Error::UnsupportedFormat` にする。
  `wBitsPerSample == 32` で `wValidBitsPerSample == 24` の 24 bit 有効値も未対応として `Error::UnsupportedFormat` にする。
- `WAVE_FORMAT_EXTENSIBLE` の `SubFormat` が上記以外の場合は `Error::UnsupportedFormat` にする。
- `WAVE_FORMAT_EXTENSIBLE` でない場合は `WAVE_FORMAT_PCM` の 16 bit を S16、`WAVE_FORMAT_IEEE_FLOAT` の 32 bit を F32、それ以外は `Error::UnsupportedFormat` にする。
  `GetMixFormat` は常に `WAVEFORMATEXTENSIBLE` を返すため、この経路は将来の呼び出しや防御のためのものだが、16 bit PCM と 32 bit float の既存の判定は変えない。
- `WAVEFORMATEXTENSIBLE` を組み立てる単体テストを追加し、8 bit PCM、16 bit PCM、24 bit PCM、32 bit PCM、32 bit float、64 bit float、未知の SubFormat、`cbSize < 22` の判定を固定する。
- 未対応形式を黙って S16 や F32 として返さない。

## Windows 実機での確認手順

1. `cargo run --example device_info` で入力・出力デバイスを確認する。`GetMixFormat` の `wFormatTag`、`wBitsPerSample`、`wValidBitsPerSample`、`SubFormat` をログ出力する一時的な確認プログラムを用意する
2. 24 bit または 32 bit PCM のデバイスで `AudioPlayback::new` を呼び、`Err(Error::UnsupportedFormat(..))` が返ることを確認する。0043 実装前は `AudioCapture::new` も同様に確認する。
   コールバックに壊れた `AudioFrame` が渡らないこと、再生バッファに壊れたデータが書き込まれないことを確認する
3. 16 bit PCM と 32 bit float のデバイスで、`AudioCapture::new` / `AudioPlayback::new` が成功し、`AudioFrame.format` が S16/F32、`AudioFrame.data.len()` が `frames * channels * bytes_per_sample` と一致することを確認する
4. 24 bit または 32 bit PCM のデバイスを用意できない場合、2 の確認は単体テストで代替する。1 と 3 は実機で確認する

## 完了条件

- `determine_audio_format` の単体テストが追加され、次の判定を固定していること:
  - `WAVE_FORMAT_PCM` の 16 bit -> `Ok(AudioFormat::S16)`
  - `WAVE_FORMAT_PCM` の 8 bit -> `Err(Error::UnsupportedFormat(..))`
  - `WAVE_FORMAT_IEEE_FLOAT` の 32 bit -> `Ok(AudioFormat::F32)`
  - `WAVE_FORMAT_IEEE_FLOAT` の 64 bit -> `Err(Error::UnsupportedFormat(..))`
  - `WAVE_FORMAT_EXTENSIBLE` の PCM 16 bit (`wBitsPerSample == 16`、`wValidBitsPerSample <= 16`) -> `Ok(AudioFormat::S16)`
  - `WAVE_FORMAT_EXTENSIBLE` の PCM 8 bit (`wBitsPerSample == 8`) -> `Err(Error::UnsupportedFormat(..))`
  - `WAVE_FORMAT_EXTENSIBLE` の PCM 24 bit (`wBitsPerSample == 24`) -> `Err(Error::UnsupportedFormat(..))`
  - `WAVE_FORMAT_EXTENSIBLE` の PCM 32 bit (`wBitsPerSample == 32`) -> `Err(Error::UnsupportedFormat(..))`
  - `WAVE_FORMAT_EXTENSIBLE` の PCM 32 bit コンテナで `wValidBitsPerSample == 24` -> `Err(Error::UnsupportedFormat(..))`
  - `WAVE_FORMAT_EXTENSIBLE` の IEEE_FLOAT 32 bit -> `Ok(AudioFormat::F32)`
  - `WAVE_FORMAT_EXTENSIBLE` の IEEE_FLOAT 64 bit -> `Err(Error::UnsupportedFormat(..))`
  - `WAVE_FORMAT_EXTENSIBLE` の未知の SubFormat -> `Err(Error::UnsupportedFormat(..))`
  - `WAVE_FORMAT_EXTENSIBLE` で `cbSize < 22` -> `Err(Error::UnsupportedFormat(..))`
  - 未知の `wFormatTag` -> `Err(Error::UnsupportedFormat(..))`
- `determine_audio_format` が `Err` を返すとき、呼び出し側 (`WasapiPlaybackImpl::new`、0043 実装前は `WasapiCaptureImpl::new` も) が `Err` を伝播し、壊れた `AudioFrame` や再生バッファを生成しないこと
- 16 bit PCM と 32 bit float の判定が変わらないこと
- Windows で `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` がパスすること
- `CHANGES.md` の `## develop` に `[CHANGE]` エントリ (`Error::UnsupportedFormat` の追加) と `[FIX]` エントリ (誤判定の修正) を追加すること
- Windows 実機で、24 bit または 32 bit PCM のデバイスが用意できる場合、`AudioPlayback::new` (0043 実装前は `AudioCapture::new` も) が `Err` を返し、壊れた音声が再生・処理へ渡らないこと (「Windows 実機での確認手順」を実施する)

## 解決方法

- `determine_audio_format` を `Result<AudioFormat>` を返す形に変更し、未対応形式を `Error::UnsupportedFormat` で明示的に返す
- `WAVE_FORMAT_EXTENSIBLE` の `cbSize` / `SubFormat` / `wBitsPerSample` / `wValidBitsPerSample` を検証する
- `WAVEFORMATEXTENSIBLE` を組み立てる単体テストを追加する
- `WasapiCaptureImpl::new` と `WasapiPlaybackImpl::new` の呼び出し側を新しい戻り値に合わせる。
  0043 が capture から `determine_audio_format` を削除するため、0043 の実装後に capture 側の呼び出しは存在しない。0045 を先に実装する場合のみ `WasapiCaptureImpl::new` も `?` に合わせる
- Windows 実機で「Windows 実機での確認手順」を実施する
