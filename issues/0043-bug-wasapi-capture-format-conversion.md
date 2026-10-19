# WASAPI キャプチャが要求したサンプルレートとチャンネル数を無視する

- Created: 2026-10-19
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-capture-format-conversion
- Polished: 2026-10-19
- Reporter: @voluntas

## 目的

Windows (WASAPI) の音声キャプチャで `AudioCaptureConfig` の `sample_rate` と `channels` がストリームに反映されない問題を修正する。
呼び出し側は指定したフォーマットでフレームが届く前提で実装しており、実際のフォーマットが違うと音声のピッチ・再生時間・チャンネル配置がずれる。

報告では、moq-pub が Windows の実マイクから送信した音声を moqt-devtools で再生すると、macOS と比べてピッチが下がり定期的に途切れる。
報告では、moq-pub は 48 kHz 1ch 固定で Opus 符号化しており、WASAPI から届くフレームは共有モードの mix format であるため、2ch 48000 Hz では 1 オクターブ下 (-12 半音)、1ch 96000 Hz でも 1 オクターブ下になる。
この issue ではピッチとフォーマットの不一致を対象とし、途切れは別要因 (0044 の timestamp の問題) と切り分ける。

## 現状

`src/capture_wasapi.rs` の `WasapiCaptureImpl::new` は、`AudioCaptureConfig` から `device_id` だけを使用し、`sample_rate` と `channels` を参照しない。

`IAudioClient::GetMixFormat` が返した mix format をそのまま `IAudioClient::Initialize` に渡すため、共有モードのオーディオエンジンの mix format (チャンネル数・サンプルレート) がそのままストリームの形式になる。
`AudioFrame.channels`、`AudioFrame.sample_rate`、`AudioFrame.format` にも mix format から得た値が入る。

- 2ch 48000 Hz のマイク: インターリーブ列をモノラルとして扱うと 1 オクターブ下になる
- 1ch 96000 Hz のマイク: 48 kHz として扱うと 1 オクターブ下になる
- 1ch 44100 Hz のマイク: 48 kHz として扱うと約 +1.5 半音上になる

`README.md` のキャプチャ例には `sample_rate: 48000` と `channels: 1` を指定するコードが載っているが、Windows で無視されることの注記は無い。

macOS の `audio_coreaudio_session_create` は要求フォーマットで `AudioQueueNewInput` を作る。
Linux の PulseAudio と PipeWire も要求フォーマットを指定し、サーバ側で変換する。
要求フォーマットを指定するのは macOS / PulseAudio / PipeWire で、WASAPI だけが指定を無視する。

## 設計方針

- `AudioCaptureConfig` の `sample_rate` と `channels` から要求フォーマット (`WAVEFORMATEX`、`WAVE_FORMAT_PCM`、S16) を構築し、`IAudioClient::Initialize` に渡す
- 共有モードで形式変換を有効にするため、`AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM` と `AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY` を付与する
- 共有モードのイベント駆動では `hnsBufferDuration` と `hnsPeriodicity` を 0 にする (Microsoft Learn の `IAudioClient::Initialize` Remarks)
- 要求フォーマットでの `Initialize` が成功した場合、`SessionData` の `format` は `AudioFormat::S16`、`sample_rate` と `channels` は `AudioCaptureConfig` の値にする。`AudioFrame` も同じ値を使う
- 要求フォーマットでの `Initialize` が失敗した場合は、mix format へフォールバックせず `Error::SessionCreateFailed` を返す。フォールバックは失敗した `IAudioClient` の再取得が必要になるため採用しない (Microsoft Learn の `IAudioClient::Initialize` Remarks)
- `IsFormatSupported` は `AUTOCONVERTPCM` を考慮しないため、要求フォーマットの事前判定には使わない。`Initialize` の成否で判断する
- `GetMixFormat` と `determine_audio_format(mix_format)` は要求フォーマット経路では使わない (capture から削除する)。`determine_audio_format` は playback 側で引き続き使う
- `AudioCaptureConfig` の `sample_rate` が 0 以下の場合は 48000、`channels` が 0 以下の場合は 1 に丸める (macOS / PulseAudio / PipeWire の既存挙動に合わせる)。
  `WAVEFORMATEX` の `u16` / `u32` フィールドに収まらない値は `Error::SessionCreateFailed` を返す
- 要求フォーマットの構築を純粋関数へ切り出し、正常値・0 以下・`u16` / `u32` 超過の単体テストを追加する
- README のキャプチャ例と `src/common.rs` の `AudioCaptureConfig` の rustdoc に、指定フォーマットが守られることとエラーになる条件を明記する
- Windows 実機での確認手順を「Windows 実機での確認手順」節に残す。CI は Windows のコンパイル確認のみで実デバイスを開かないため、自動テストでは確認できない

## Windows 実機での確認手順

1. `cargo run --example device_info` で入力デバイスの mix format (チャンネル数・サンプルレート) を確認する。2ch 48000 Hz と 1ch 96000 Hz の入力デバイスを用意する
2. `AudioCaptureConfig { device_id: None, sample_rate: 48000, channels: 1 }` で 5 秒間キャプチャする一時的な確認プログラムを実行し、各 `AudioFrame` の `channels`、`sample_rate`、`format`、`frames`、`data.len()` をログ出力する
3. `channels` が 1、`sample_rate` が 48000、`format` が `AudioFormat::S16`、`data.len()` が `frames * 1 * 2` になっていることを確認する
4. moq-pub で 2ch 48000 Hz のマイクから送信し、moqt-devtools で再生して、macOS と比べてピッチが一致することを確認する。1ch 96000 Hz のマイクでも同じ確認をする
5. 要求フォーマットを意図的に非対応にする (例: `sample_rate = 1`) か、`channels` を `WAVEFORMATEX` の `u16` に収まらない値にして、`Error::SessionCreateFailed` が返り、mix format へ黙ってフォールバックしないことを確認する

## 完了条件

- 2ch 48000 Hz のマイクを `sample_rate = 48000`, `channels = 1` で開いたとき、`AudioFrame.channels` が 1、`AudioFrame.sample_rate` が 48000、`AudioFrame.format` が `AudioFormat::S16`、
  `AudioFrame.data.len()` が `frames * 1 * 2` になること
- 1ch 96000 Hz のマイクを `sample_rate = 48000`, `channels = 1` で開いたときも、48000 Hz 1ch の S16 フレームが届き、`AudioFrame.data.len()` が `frames * 1 * 2` になること
- 要求フォーマットでの `Initialize` が失敗した場合に `Error::SessionCreateFailed` を返し、mix format へ黙ってフォールバックしないこと (実機確認で固定)
- 要求フォーマット構築の純粋関数の単体テストが追加され、正常値、0 以下、`u16` / `u32` 超過を固定していること
- README と `AudioCaptureConfig` の rustdoc が実装と一致していること
- Windows 実機で、修正前はピッチが下がっていた音声が正しいピッチで再生されること (「Windows 実機での確認手順」を実施する)

## 解決方法

- `WasapiCaptureImpl::new` で `AudioCaptureConfig` から要求フォーマットを構築し、`Initialize` に渡すフラグとバッファ長を変更する
- `SessionData` と `AudioFrame` の `format` を `AudioFormat::S16`、`sample_rate` と `channels` を `AudioCaptureConfig` の値にする
- `GetMixFormat` と `determine_audio_format(mix_format)` を capture から削除する
- 要求フォーマットの構築を純粋関数へ切り出し、正常値、0 以下、`u16` / `u32` 超過を含む単体テストを追加する
- 要求フォーマットでの `Initialize` が失敗した場合は `Error::SessionCreateFailed` を返す (フォールバックしない)
- README のキャプチャ例と `src/common.rs` の `AudioCaptureConfig` の rustdoc に、Windows のフォーマット変換とエラー条件を追加する
- Windows 実機で「Windows 実機での確認手順」を実施する
