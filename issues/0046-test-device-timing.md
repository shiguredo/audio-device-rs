# 実機のオーディオキャプチャと再生のタイミングを検証するテストを追加する

- Created: 2026-10-24
- Completed: 2026-10-25
- Branch: feature/add-device-timing-tests
- Polished: {YYYY-MM-DD}

## 目的

実機のオーディオキャプチャと再生を使い、`cargo test` で時間の扱いが正しいことを検証できるようにする。
純粋関数のテストだけでは、実際のデバイスとの統合、パケットの到着間隔、デバイスごとの差異を検証できない。
`cargo test` の実機ランナーで、利用可能な入力デバイスと出力デバイスを列挙し、それぞれについて検証するプロセスを確立する。

## 現状

- `src/capture_wasapi.rs` の `#[cfg(test)] mod tests` は `timestamp_for_packet` などの純粋関数だけをテストしている。
- `tests/test_capture.rs` は FFI バックエンドのデバイス ID 検証だけで、Windows ではコンパイルされない。
- `.github/workflows/ci.yml` の `device-test` ジョブは実機ランナーで `cargo test --workspace` と `device_list` / `device_info` を実行するが、キャプチャや再生を開始しない。
- `AudioFrame.timestamp_us` の実機確認は手動で行われているだけで、リポジトリに自動テストがない。
- `PlaybackFrame` にはタイムスタンプがなく、`AudioPlayback` のコールバックは要求フレーム数・チャンネル数・サンプルレートだけを受け取る。実際の出力タイミングを公開 API だけで直接測ることはできない。

## 設計方針

- `tests/test_device_timing.rs` を追加し、常に `cargo test` の対象にする。`AudioDeviceList::enumerate_input()` と `AudioDeviceList::enumerate_output()` で取得した全てのデバイスを対象とし、デバイスが無い環境ではスキップする。
- キャプチャ:
  - 各入力デバイスで `AudioCapture` を構築し、数秒間キャプチャする。
  - 1 つ以上の `AudioFrame` が届くことを確認する。
  - `timestamp_us` が単調に増加することを確認する。
  - 連続するフレームの `timestamp_us` の差が、直前のフレームの `frames / sample_rate` と ±1 ms 以内で一致することを確認する。パケット欠落が発生した場合は、その区間を検出してスキップするか、許容する割合を決める。
- 再生:
  - 各出力デバイスで `AudioPlayback` を構築し、生成した信号を数秒間再生する。
  - コールバックが呼ばれ、要求フレーム数が増えることを確認する。
  - 要求フレーム数の累積と経過時間を比較し、バッファ長を考慮した許容範囲で極端な進み・遅れがないことを確認する。
  - 実際の DAC 出力のタイミング検証は、ループバックキャプチャかハードウェアループバックが必要であり、この issue の対象外とする。必要なら別 issue に分ける。
- デバイスが無い場合とエラーの扱い:
  - `enumerate_input()` または `enumerate_output()` が 0 件を返した場合は、オーディオバックエンドやデバイスが無い環境とみなして、その方向のテストをスキップする。Rust のテストハーネスには skip がないため、理由をログ出力して早期 return し、テストは成功扱いにする。
  - `enumerate_input()` または `enumerate_output()` がエラーを返した場合は、デバイス名とエラーを出力してテストを失敗させる。
  - デバイスが 1 件以上列挙された後のオープン、キャプチャまたは再生の開始、フレーム受信でエラーが発生した場合は、デバイス名とエラーを出力してテストを失敗させる。
  - デバイスはあるがキャプチャフレームが 1 つも届かない場合は、入力環境の不備としてテストを失敗させる。
- 実行環境:
  - Windows / Linux / macOS の `ci` と `device-test` ジョブで `cargo test --workspace` を実行する。
  - 組み込みランナーではデバイスが無いためスキップされ、self-hosted ランナーではデバイスがある場合に検証される。
  - Linux は PulseAudio と PipeWire の両方の CI ジョブがあるため、それぞれで実行するか、デフォルトバックエンドのみとするかを決める。
  - ホステッドランナーでは PulseAudio / PipeWire のサーバーだけを起動し、デバイスが無い場合はスキップする。実機デバイスを持つ self-hosted ランナーで検証する。

## 完了条件

- `cargo test --workspace` で、実機の入力デバイスと出力デバイスを使ったテストが実行される
- デバイスが 0 個の場合は、その方向のテストをスキップして成功する
- デバイス列挙がエラーの場合は、テストが失敗する
- デバイスが列挙された後のオープン、開始、フレーム受信のエラーはテストが失敗する
- 入力デバイスごとに `timestamp_us` の単調性と音声時間との整合性が検証される
- 出力デバイスごとに再生コールバックと要求フレーム数の推移が検証される
- デバイスがある環境では Windows / Linux / macOS で実機テストが実行され、デバイスが無い環境ではスキップして成功する

## 解決方法

- `tests/test_device_timing.rs` を追加し、列挙した入力デバイスで `AudioFrame.timestamp_us` の単調性と音声時間との整合性を検証するようにした
- 列挙した出力デバイスで再生コールバックの呼び出しと要求フレーム数の累積を検証するようにした
- デバイスが 0 件の場合はスキップし、列挙エラーとデバイスを開いた後のエラーは失敗するようにした
- PulseAudio と PipeWire のキャプチャで `AudioFrame.timestamp_us` をパケット先頭の録音時刻に修正した
- デバイスが 0 件のときに `Error::DeviceAccessDenied` を返していた問題を修正した
- CI では PulseAudio と PipeWire のサーバーだけを起動し、デバイスが無い場合はスキップするようにした
- Device Test の実機ランナーで Windows / Linux / macOS の実デバイスを検証した
