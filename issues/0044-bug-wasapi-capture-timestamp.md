# WASAPI キャプチャの timestamp が読み出し時刻になっている

- Created: 2026-10-19
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-capture-timestamp
- Polished: 2026-10-19

## 目的

Windows (WASAPI) の音声キャプチャで、`AudioFrame.timestamp_us` をパケット先頭フレームのキャプチャ時刻にする。
現状はパケットを読み出した時刻 (クライアントの処理時刻) であり、読み出しが遅れたり複数パケットをまとめて読んだりすると実際の音声の時刻とずれる。
音声と映像の同期、および受信側の再生タイミングが崩れる。

報告では、moq-pub が Windows の実マイクから送信した音声を moqt-devtools で再生すると、macOS と比べて途切れる。
0043 はピッチとフォーマットの不一致を対象とし、途切れは本 issue の timestamp の問題として切り分けている。

## 現状

`src/capture_wasapi.rs` の `capture_thread_func` は、`IAudioCaptureClient::GetBuffer` の `pu64DevicePosition` と `pu64QPCPosition` に `None` を渡して捨てている。
その後 `QueryPerformanceCounter` を読み、`qpc_counter_to_micros` で変換した値を `timestamp_us` にしている。

- この値はパケットを読み出した時刻であり、デバイスが先頭フレームを記録した時刻ではない
- 1 回の読み出しで複数パケットを処理すると、各パケットの timestamp がマイクロ秒差のほぼ同時刻になる。この実装は 10 ms バッファを指定しているためパケットは典型的には 10 ms 間隔で届くが、パケット長は可変であり、パケットの音声時間 (`frames / sample_rate`) の間隔が失われる
- macOS の `audio_input_callback` は `AudioTimeStamp.mHostTime` (バッファ先頭のキャプチャ時刻) を使っており、読み出し時刻ではない
- `flags` は現在 `AUDCLNT_BUFFERFLAGS_SILENT` だけを参照しており、`AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` と `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY` は扱っていない

[`IAudioCaptureClient::GetBuffer`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer) の `pu64QPCPosition` は、パケット先頭フレームのデバイス位置を記録した時点の
パフォーマンスカウンタを 100 ナノ秒単位へ換算した値である (Microsoft Learn)。
生の QPC 値を `t`、`QueryPerformanceFrequency` の値を `f` とすると `10,000,000 * t / f` で計算され、現在の `QueryPerformanceCounter` と同じ QPC の起動時基準だが、単位は 100 ナノ秒である。

## 設計方針

- `capture_thread_func` の `GetBuffer` 呼び出しで、第 4 引数 `pu64DevicePosition` と第 5 引数 `pu64QPCPosition` の両方を受け取る。`qpc_position` は 0、`last_anchor` と `last_timestamp_us` は `None` で初期化する
- `pu64QPCPosition` は API が 100 ナノ秒単位へ換算済みの値である。`QueryPerformanceFrequency` を使った再換算はせず、`qpc_position / 10` をマイクロ秒として使う
- `TimestampAnchor` は補間に使う直前のパケットの基準を表す。`device_position: u64` はストリーム開始からのサンプルフレーム数、`timestamp_us: i64` はそのパケット先頭のキャプチャ時刻 (マイクロ秒) である
- パケットごとの timestamp 決定を純粋関数 `timestamp_for_packet` へ切り出す。引数は `flags: u32`, `qpc_position: u64`, `device_position: u64`,
  `sample_rate: i32`, `read_time_us: i64`, `last_anchor: Option<TimestampAnchor>`, `last_timestamp_us: Option<i64>`、戻り値は
  `(timestamp_us: i64, new_anchor: Option<TimestampAnchor>)` とする
- `last_timestamp_us` は `capture_thread_func` がコールバックへ渡した直前のパケットのタイムスタンプである
- `read_time_us` は `QueryPerformanceCounter` と `qpc_counter_to_micros` で計算し、`timestamp_for_packet` に渡す
- `timestamp_for_packet` の決定規則:
  - `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` が立っていない場合: `qpc_position_to_micros(qpc_position)` を timestamp にし、`TimestampAnchor { device_position, timestamp_us }` を新しい基準にする
  - `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` が立つ場合:
    `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY` が立っておらず直前の基準 `last_anchor` があるなら `interpolate_timestamp_us` で補間する。
    基準がない、`sample_rate <= 0`、`device_position < last_device_position`、計算オーバーフローの場合は読み出し時刻 (`read_time_us`) へフォールバックし、基準を `None` にする
  - `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY` が立つ場合: 基準をリセットする。`AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` が立っていなければ QPC を使い、そうでなければ読み出し時刻へフォールバックする
  - 読み出し時刻へフォールバックしたパケットでは基準を更新しない (基準を読み出し時刻で汚染しないため)
- `qpc_position` の有効性は `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` だけで判定する。`qpc_position == 0` は特別扱いしない
- `timestamp_for_packet` は、戻り値の `timestamp_us` が `last_timestamp_us` 以下にならないよう、必要なら直前の値 + 1 マイクロ秒にクランプする。基準の `TimestampAnchor.timestamp_us` はクランプ前の音声時刻のままにする
- `qpc_position_to_micros(qpc_position: u64) -> i64` は `(qpc_position / 10) as i64` を返す。`u64::MAX / 10` は `i64` の範囲に収まる
- `interpolate_timestamp_us(last_timestamp_us: i64, last_device_position: u64, device_position: u64, sample_rate: i32) -> Option<i64>` は、
  `sample_rate <= 0`、`device_position < last_device_position`、計算オーバーフローの場合に `None` を返す
- `qpc_counter_to_micros` は読み出し時刻へのフォールバックで使い続けるため削除しない。既存の `frequency <= 0` のテストは維持する
- `AUDCLNT_BUFFERFLAGS_SILENT` の扱い (無音パケットをコールバックしない) は変更しない
- 変更対象は `src/capture_wasapi.rs` の `capture_thread_func` と `#[cfg(test)] mod tests` に限定する。`AudioFrame` の型や他バックエンドは変更しない

## Windows 実機での確認手順

1. `cargo run --example device_info` で入力デバイスを確認する
2. `initialize_thread()` を呼び、`AudioCaptureConfig { device_id: None, sample_rate: 48000, channels: 1 }` で 5 秒間キャプチャする一時的な確認プログラムを実行し、各 `AudioFrame` の `timestamp_us`、`frames`、`sample_rate` をログ出力する
3. 連続するパケットの `timestamp_us` の差が、直前のパケットの `frames / sample_rate` から計算した時間と ±1 ms 以内で一致し、単調に増えることを確認する
4. コールバック内で意図的に 20 ms 待つなどして読み出しを遅らせ、複数パケットが溜まった状態で `GetBuffer` ループを回す。
   各パケットの `timestamp_us` が同時刻にならず、`pu64DevicePosition` の差 / `sample_rate` と ±1 ms 以内で一致することを確認する。
   `pu64DevicePosition` と `pu64QPCPosition` は `capture_thread_func` に一時的なデバッグログを追加して記録し、確認後に削除する
5. moq-pub で送信し、moqt-devtools で 5 分間再生して、修正前に発生していた途切れが再現しないことを確認する。音声と映像を同時に扱う場合は、映像との同期が修正前より安定することを確認する
6. `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` の実機発生は必須としない。分岐は手順 3〜4 の正常系と、`timestamp_for_packet` の単体テストで固定する

## 完了条件

- `timestamp_for_packet` の単体テストが追加され、次の分岐を固定していること:
  - 通常時 (`AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` なし): `qpc_position / 10` を返し、基準を更新する
  - `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` あり、基準あり: 補間値を返し、基準を更新する
  - `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` あり、基準なし: 読み出し時刻を返し、基準を `None` にする
  - `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY` あり: 基準をリセットし、QPC が有効なら QPC、そうでなければ読み出し時刻を返す
  - `sample_rate <= 0`、`device_position < last_device_position`、計算オーバーフロー: 読み出し時刻を返す
- `timestamp_for_packet` の単体テストで、戻り値の `timestamp_us` が `last_timestamp_us` 以下にならず、基準の `timestamp_us` はクランプ前の音声時刻のままであること
- `clamp_timestamp_us` の単体テストが追加され、直前の値がない場合、直前より大きい場合、直前以下の場合、`i64::MAX` で正しい値を返すこと
- `qpc_position_to_micros` の単体テストが追加され、0、10 ms、`u64::MAX` で正しいマイクロ秒を返すこと
- `interpolate_timestamp_us` の単体テストが追加され、正常な 10 ms 間隔、`sample_rate <= 0`、`device_position` の逆転、計算オーバーフローで `None` を返すこと
- `qpc_counter_to_micros` の既存テスト (`returns_zero_when_frequency_is_not_positive` など) が維持されていること
- Windows 実機で、通常時の `AudioFrame.timestamp_us` が `pu64QPCPosition / 10` と一致すること
- Windows 実機で、キャプチャ中に、連続するパケットの `timestamp_us` の差が直前のパケットの `frames / sample_rate` から計算した時間と ±1 ms 以内で一致し、単調に増えること
- 1 回のイベント待機後のループで複数パケットを処理したとき、各パケットの `timestamp_us` の差が、`pu64DevicePosition` の差 / `sample_rate` と ±1 ms 以内で一致すること (同時刻にならないこと)
- Windows 実機で、「Windows 実機での確認手順」を実施し、burst 読み出しで同時刻にならず、moq-pub → moqt-devtools の再生で修正前に比べて途切れが再現しないこと
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` がパスすること (Windows を含む)

## 解決方法

- `capture_thread_func` の `GetBuffer` 呼び出しで `pu64DevicePosition` と `pu64QPCPosition` を受け取り、`timestamp_us` に使う
- `timestamp_for_packet`、`qpc_position_to_micros`、`interpolate_timestamp_us` を純粋関数として追加し、単体テストを追加する
- `AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR` と `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY` の扱いを実装する
- `timestamp_for_packet` 内で、戻り値の `timestamp_us` が `last_timestamp_us` 以下にならないよう `clamp_timestamp_us` でクランプする
- `qpc_counter_to_micros` は読み出し時刻へのフォールバック用として残す
- Windows 実機で「Windows 実機での確認手順」を実施する
