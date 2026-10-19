# WASAPI キャプチャが要求したサンプルレートとチャンネル数を無視する

- Created: 2026-10-19
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-capture-format-conversion
- Polished: {YYYY-MM-DD}
- Reporter: @voluntas

## 目的

Windows (WASAPI) の音声キャプチャで `AudioCaptureConfig` の `sample_rate` と `channels` がストリームに反映されない問題を修正する。
呼び出し側は指定したフォーマットでフレームが届く前提で実装しており、実際のフォーマットが違うと音声のピッチ・再生時間・チャンネル配置がずれる。

実際に、moq-pub が Windows の実マイクから送信した音声を moqt-devtools で再生すると、macOS と比べてピッチが下がり定期的に途切れる。
moq-pub は 48 kHz 1ch 固定で Opus 符号化するが、WASAPI から届くフレームはデバイスの mix format であり、2ch 48000 Hz では 1 オクターブ下 (-12 半音)、1ch 96000 Hz でも 1 オクターブ下になる。

## 現状

`src/capture_wasapi.rs` の `WasapiCaptureImpl::new` は、`AudioCaptureConfig` から `device_id` だけを使用し、`sample_rate` と `channels` を参照しない。

`IAudioClient::GetMixFormat` が返した mix format をそのまま `IAudioClient::Initialize` に渡すため、共有モードのデバイス Default Format (チャンネル数・サンプルレート) がそのままストリームの形式になる。
`AudioFrame.channels` と `AudioFrame.sample_rate` にも mix format の値が入る。

- 2ch 48000 Hz のマイク: インターリーブ列をモノラルとして扱うと 1 オクターブ下になる
- 1ch 96000 Hz のマイク: 48 kHz として扱うと 1 オクターブ下になる
- 1ch 44100 Hz のマイク: 48 kHz として扱うと約 +1.5 半音上になる

`README.md` のキャプチャ例には `sample_rate: 48000` と `channels: 1` を指定するコードが載っているが、Windows で無視されることの注記は無い。

macOS の `audio_coreaudio_session_create` は要求フォーマットで `AudioQueueNewInput` を作り、AudioQueue が変換する。
Linux の PulseAudio と PipeWire も要求フォーマットを指定し、サーバ側で変換する。
要求フォーマットが無視されるのは WASAPI だけである。

## 設計方針

- `AudioCaptureConfig` の `sample_rate` と `channels` から要求フォーマット (`WAVEFORMATEX`、S16) を構築し、`IAudioClient::Initialize` に渡す
- 共有モードで形式変換を有効にするため、`AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM` と `AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY` を付与する
- 共有モードのイベント駆動では `hnsBufferDuration` と `hnsPeriodicity` を 0 にする (Microsoft Learn の `IAudioClient::Initialize` Remarks)
- `AUTOCONVERTPCM` に対応しない環境では mix format へフォールバックするか明示的なエラーを返す。黙って mix format を返す現在の挙動は廃止する
- `AudioCaptureConfig` の値が 0 以下の場合の扱いを決め、テストする
- README のキャプチャ例と API ドキュメントに、指定フォーマットが守られることとフォールバックする条件を明記する
- Windows 実機での確認手順を issue に残す。CI は Windows のコンパイル確認のみで実デバイスを開かないため、自動テストでは確認できない

## 完了条件

- 2ch 48000 Hz のマイクを `sample_rate = 48000`, `channels = 1` で開いたとき、`AudioFrame.channels` が 1、`AudioFrame.sample_rate` が 48000 になること
- 1ch 96000 Hz のマイクを `sample_rate = 48000`, `channels = 1` で開いたときも 48000 Hz 1ch のフレームが届くこと
- `AUTOCONVERTPCM` 非対応時の挙動 (フォールバックまたはエラー) が実装され、テストまたは実機確認で固定されていること
- README と API ドキュメントが実装と一致していること
- Windows 実機で、修正前はピッチが下がっていた音声が正しいピッチで再生されること

## 解決方法

- `WasapiCaptureImpl::new` で `AudioCaptureConfig` から要求フォーマットを構築し、`Initialize` に渡すフラグとバッファ長を変更する
- 要求フォーマットの構築を純粋関数へ切り出し、`sample_rate` と `channels` の異常値を含む単体テストを追加する
- `AUTOCONVERTPCM` が使えない場合のフォールバックまたはエラー処理を追加する
- README のキャプチャ例に Windows のフォーマット変換の説明を追加する
- Windows 実機で 2ch 48000 Hz と 1ch 96000 Hz のデバイスを使って確認する
