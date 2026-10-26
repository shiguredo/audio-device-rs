# 変更履歴

- UPDATE
  - 後方互換がある変更
- ADD
  - 後方互換がある追加
- CHANGE
  - 後方互換のない変更
- FIX
  - バグ修正

## develop

- [CHANGE] macOS / Linux で NUL バイトを含むデバイス ID を指定した場合に `Error::NullPointer` ではなく `Error::InvalidDeviceId` を返すようにする
  - @melpon
- [CHANGE] Windows ではライブラリを利用するスレッドの先頭で `initialize_thread()` を呼ぶ必要があるようにする
  - @melpon
- [CHANGE] `Error::InvalidAudioFormat` を追加する
  - WASAPI で指定したサンプルレートやチャンネル数が音声形式として表現できない場合は `Error::SessionCreateFailed` ではなく `Error::InvalidAudioFormat` を返す
  - @melpon
- [CHANGE] `AudioCapture::stop` と `AudioPlayback::stop` が `Result<()>` を返すように変更する
  - 停止に失敗した場合は `Error::SessionStopFailed` を返す
  - コールバックからの stop は停止処理を行わず `Error::SessionStopFailed` を返す
  - `Error::SessionStopFailed` を追加する
  - @melpon
- [CHANGE] オーディオコールバックからの破棄を禁止する
  - コールバック上で破棄した場合は panic する
  - @melpon
- [FIX] `Cargo.toml` の `windows` 依存を MSRV 1.93 の cargo が解釈できる形式に修正する
  - @voluntas
- [FIX] examples が `--no-default-features` でビルドできない問題を修正する
  - オーディオバックエンドが有効でない場合はエラーメッセージを表示して終了する
  - @voluntas
- [FIX] WASAPI キャプチャのタイムスタンプ計算で i64 オーバーフローが発生する問題を修正する
  - @melpon
- [FIX] WASAPI キャプチャの Use-After-Free と、スレッド生成失敗時に running フラグが残る問題を修正する
  - @melpon
- [FIX] WASAPI で COM の初期化カウンタが解放されない問題を修正する
  - @melpon
- [FIX] WASAPI 再生で `GetBuffer` 失敗時にコールバックが取得したフレームデータが破棄される問題を修正する
  - @melpon
- [FIX] `write_playback_frame_to_buffer` が `dst_channels` 0 のときゼロ除算でパニックする問題を修正する
  - 変換パスでもフレーム境界に満たないサンプルを書き込まないようにする
  - @melpon
- [FIX] `PlaybackFrame::from_s16` / `from_f32` でデータ長がチャンネル数の倍数でない場合に `Error::InvalidDataLength` を返すようにする
  - `frames` と実データ量が食い違う `PlaybackFrame` を生成しないようにする
  - @melpon
- [FIX] WASAPI キャプチャで `AudioCaptureConfig` の `sample_rate` と `channels` を無視する問題を修正する
  - 指定した形式へ変換できない場合は `Error::SessionCreateFailed` を返す
  - @melpon
- [FIX] WASAPI キャプチャの `AudioFrame.timestamp_us` をパケット先頭フレームのキャプチャ時刻にする
  - `IAudioCaptureClient::GetBuffer` の QPC 位置を使い、利用できない場合はデバイス位置から補間する
  - @melpon
- [FIX] WASAPI 再生でデバイスの mix format に依存せず、常に F32 で初期化するようにする
  - 24/32 bit PCM のデバイスでも壊れた音声データを生成しない
  - @melpon
- [FIX] WASAPI 再生で `AudioPlaybackConfig` の `sample_rate` と `channels` を無視する問題を修正する
  - 0 以下の場合は 48000 Hz / 2 ch を使う
  - @melpon
- [FIX] PulseAudio キャプチャの `AudioFrame.timestamp_us` をパケット先頭フレームの録音時刻にする
  - `pa_stream_get_latency` から求めた録音時刻を基準に、パケットの音声時間で補間する
  - @melpon
- [FIX] PipeWire キャプチャの `AudioFrame.timestamp_us` をバッファのキャプチャ時刻にする
  - `pw_buffer.time` を使い、利用できない場合はストリーム時刻へフォールバックする
  - @melpon
- [FIX] デバイスが 0 件のときに `Error::DeviceAccessDenied` を返す問題を修正する
  - 空のデバイスリストを正常な結果として返す
  - @melpon
- [FIX] PipeWire 再生でバッファ容量ではなく PipeWire が要求したサンプル数を使うように修正する
  - @melpon
- [FIX] PipeWire 再生セッションの開始で `pw_stream_new` が失敗したときに `pw_properties` を二重解放する問題を修正する
  - @melpon
- [FIX] PipeWire のキャプチャ・再生セッション開始で、ストリーム状態の待機がタイムアウトせずハングする問題を修正する
  - ストリームが STREAMING または PAUSED に遷移しないまま約 10 秒 (待機の粒度により最大約 11 秒) 経過した場合は、セッション開始に失敗する
  - タイムアウトは CLOCK_MONOTONIC の経過時間で判定する。待機には CLOCK_REALTIME を使うため、システム時刻の後退やサスペンドがある環境ではこの上限は保証されない
  - @melpon
- [FIX] PipeWire のキャプチャ・再生で、stop 後に start を再実行すると SIGSEGV する問題を修正する
  - stop では stream を切断して thread loop を停止してから stream を破棄し、core_listener を remove して core を切断する
  - @melpon
- [FIX] PulseAudio のキャプチャ・再生で、stop 後に start を再実行すると abort する問題を修正する
  - start ごとに mainloop / context / stream を作成し、stop でまとめて破棄する
  - @melpon

### misc

- [ADD] デフォルトバックエンドのセッション開始・停止・再開始がハング・クラッシュしないことを検証するテストを追加する
  - process コールバックから stop を呼び出すテストを含む
  - @melpon
- [ADD] 実機の入力デバイスと出力デバイスでキャプチャと再生のタイミングを検証するテストを追加する
  - @melpon
- [ADD] `write_playback_frame_to_buffer` の直接テストを追加する
  - @voluntas
- [UPDATE] PulseAudio のセッション開始はサーバが応答しない場合に無期限にブロックすることをドキュメントに記載する
  - @melpon
- [UPDATE] PBT をプロパティベーステストフレームワーク noprop に移行する
  - @voluntas
- [UPDATE] shiguredo-rust 規約に準拠する
  - 公開 API のドキュメント追加、`Error` への `Copy` 実装、不要な lint 抑制の削除、`.unwrap()` から `.expect()` への置き換えなど
  - @voluntas
- [UPDATE] `rust-toolchain.toml` のツールチェーンを MSRV (1.93) に固定する
  - @voluntas
- [UPDATE] 依存クレートのバージョン指定をマイナーバージョンまでに揃え、用途コメントを追加する
  - @voluntas
- [UPDATE] `AudioDeviceList` の列挙メソッドの重複コードを共通ヘルパーに集約する
  - @melpon

## 2026.3.0

**リリース日**: 2026-07-30

- [CHANGE] MSRV (rust-version) を 1.93 に上げる
  - @voluntas

## 2026.2.0

**リリース日**: 2026-07-22

- [CHANGE] AudioDevice, AudioDeviceList, AudioCapture, AudioPlayback を enum newtype 化し pulse と pipewire を共存可能にする
  - @melpon
- [ADD] macOS / Linux に音声再生機能を追加する
  - @melpon
- [FIX] C コード内の strdup 戻り値 NULL チェック欠落を修正する
  - @melpon
- [FIX] from_ffi() が未知の FFI 定数値を無条件に Input/S16 へ丸め込む問題を修正する
  - @melpon
- [FIX] PulseAudio デバイス列挙でコンテキスト切断時に無限ループする問題を修正する
  - @melpon
- [FIX] PlaybackFrame::from_s16/from_f32 での usize から i32 へのキャストによる整数オーバーフローを防止する
  - @melpon
- [FIX] pw_init の戻り値未チェックを修正する
  - @melpon

### misc

- [ADD] cargo-fuzz を用いた fuzzing ターゲットを追加する
  - @melpon
- [ADD] Error 型の PBT テストを追加する
  - @melpon
- [UPDATE] プラットフォーム間で重複していた共通型を src/common.rs に抽出する
  - @melpon
- [FIX] SendHandle / SendPtr の重複定義を共通化する
  - @melpon
- [FIX] determine_audio_format の重複定義を統合する
  - @melpon
- [FIX] examples のエラーメッセージを日本語から英語に修正する
  - @melpon
- [FIX] PBT から「パニックしないこと」のみを検証するテストを削除する
  - @melpon

## 2026.1.0

**リリース日**: 2026-04-03
