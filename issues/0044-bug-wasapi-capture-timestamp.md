# WASAPI キャプチャの timestamp が読み出し時刻になっている

- Created: 2026-10-19
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-capture-timestamp
- Polished: {YYYY-MM-DD}

## 目的

Windows (WASAPI) の音声キャプチャで、`AudioFrame.timestamp_us` をパケット先頭フレームのキャプチャ時刻にする。
現状はパケットを読み出した時刻 (クライアントの処理時刻) であり、読み出しが遅れたり複数パケットをまとめて読んだりすると実際の音声の時刻とずれる。
音声と映像の同期、および受信側の再生タイミングが崩れる。

## 現状

`src/capture_wasapi.rs` の `capture_thread_func` は、`IAudioCaptureClient::GetBuffer` の `pu64DevicePosition` と `pu64QPCPosition` に `None` を渡して捨てている。
その後 `QueryPerformanceCounter` を読み、`qpc_counter_to_micros` で変換した値を `timestamp_us` にしている。

- この値はパケットを読み出した時刻であり、デバイスが先頭フレームを記録した時刻ではない
- 1 回の読み出しで複数パケットを処理すると、各パケットの timestamp がマイクロ秒差のほぼ同時刻になる。実際の音声は 10 ms 間隔であるため、メディア時刻が圧縮される
- macOS の `audio_input_callback` は `AudioTimeStamp.mHostTime` (バッファ先頭のキャプチャ時刻) を使っており、読み出し時刻ではない

`IAudioCaptureClient::GetBuffer` の `pu64QPCPosition` は、パケット先頭フレームの時点のパフォーマンスカウンタを 100 ナノ秒単位へ換算した値である (Microsoft Learn)。
現在の `QueryPerformanceCounter` と同じ起動時基準なので、置き換えても時刻の基準は変わらない。

## 設計方針

- `GetBuffer` の第 5 引数へ `pu64QPCPosition` を受け取り、100 ナノ秒単位の値を 10 で割ってマイクロ秒にする
- `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` が立つ場合は `pu64QPCPosition` が不正であるため、`pu64DevicePosition` とストリームのサンプルレートから補間するか、読み出し時刻へフォールバックする。どちらを選ぶかはエラー頻度と補間のずれで決める
- 変換処理を純粋関数へ切り出し、100 ナノ秒からマイクロ秒への換算と `frequency` が 0 以下の場合のテストを追加する
- `qpc_counter_to_micros` は timestamp の換算に使わなくなるため、残す場合は用途を明確にする

## 完了条件

- 10 ms 間隔で届くパケットの `timestamp_us` が 10 ms 間隔で単調に増えること
- 1 回の読み出しで複数パケットを処理しても、それぞれの `timestamp_us` がパケットの音声時刻を表すこと
- `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` が立った場合の挙動が実装され、テストまたは実機確認で固定されていること
- Windows 実機で、音声と映像の同期、および受信側の再生タイミングが改善すること

## 解決方法

- `capture_thread_func` の `GetBuffer` 呼び出しで `pu64QPCPosition` を受け取り、`timestamp_us` に使う
- `pu64QPCPosition` からマイクロ秒への換算を純粋関数へ切り出し、単体テストを追加する
- `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` の扱いを実装する
- Windows 実機で 10 ms 間隔のパケットの timestamp を記録し、読み出しの burst で同時刻にならないことを確認する
