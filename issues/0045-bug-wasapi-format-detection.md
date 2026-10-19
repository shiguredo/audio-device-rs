# determine_audio_format が 24/32 bit PCM を S16 と誤判定する

- Created: 2026-10-19
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-format-detection
- Polished: {YYYY-MM-DD}

## 目的

Windows (WASAPI) の `determine_audio_format` が `WAVE_FORMAT_EXTENSIBLE` の非 float (PCM の 24/32 bit) を S16 と誤判定する問題を修正する。
誤判定すると `AudioFrame` の `data` が実際のバッファより短くなり、波形が壊れる。
壊れたデータが音声の再生や処理へ渡るのを防ぐ。

## 現状

`src/device_wasapi.rs` の `determine_audio_format` は、`WAVE_FORMAT_IEEE_FLOAT` と `WAVE_FORMAT_EXTENSIBLE` の float SubFormat だけを F32 と判定し、それ以外をすべて S16 と判定する。
`AudioFormat` は S16 と F32 の 2 値しか持たない。

`src/capture_wasapi.rs` の `capture_thread_func` は `frames * channels * bytes_per_sample` で `data_size` を計算する。
S16 と誤判定された 24 bit PCM では実際の 3 分の 2、32 bit PCM では 2 分の 1 のバイト列だけを i16 として解釈し、残りを捨てる。
`AudioFrame::as_s16` は `data.len() >= frames * channels * 2` を満たすため、この不整合はエラーにならずにすり抜ける。

`GetMixFormat` が常に float32 を返す保証は一次資料に無い。
共有モードの mix format は通常 float32 だが、float 以外が返る環境では壊れた音声になる。
`src/playback_wasapi.rs` も同じ `determine_audio_format` を使うため、再生側にも同じ問題がある。

## 設計方針

- `WAVE_FORMAT_EXTENSIBLE` の場合は `cbSize` を検証し、`SubFormat` が PCM のときに `wBitsPerSample` と `wValidBitsPerSample` を確認する
- 24/32 bit PCM を `AudioFormat` に追加して正しく返すか、未対応として明示的なエラーを返す。`AudioFormat` を拡張する場合は `as_s16` / `as_f32` と `bytes_per_sample` の扱いを揃える
- `WAVEFORMATEX` が `WAVE_FORMAT_EXTENSIBLE` でない場合も `wBitsPerSample` を検証し、16 bit 以外を S16 と判定しない
- `WAVEFORMATEXTENSIBLE` を組み立てる単体テストを追加し、16/24/32 bit PCM と float の判定を固定する
- 未対応形式を黙って S16 として返さない

## 完了条件

- 24 bit / 32 bit PCM の `WAVE_FORMAT_EXTENSIBLE` を渡したとき、壊れた `AudioFrame` を返さないこと
- 未対応形式の場合にエラーを返すか、正しい `AudioFormat` を返すこと
- 16 bit PCM と 32 bit float の判定が変わらないこと
- 単体テストが追加され、`cargo test` がパスすること

## 解決方法

- `determine_audio_format` を `Result<AudioFormat>` を返す形に変更し、未対応形式を明示的に扱う
- `WAVE_FORMAT_EXTENSIBLE` の `cbSize` / `SubFormat` / `wBitsPerSample` を検証する
- `WAVEFORMATEXTENSIBLE` を組み立てる単体テストを追加する
- `capture_thread_func` と `playback_wasapi.rs` の呼び出し側を新しい戻り値に合わせる
