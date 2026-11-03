# 公開 API の契約を rustdoc に明記する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-public-api-docs
- Polished: 2026-11-04

## 目的

公開 API を rustdoc だけで正しく使えるように、エラー・`None`・スレッド要件・ブロック条件などの契約を明記する。

## 現状

- `src/device.rs` の `AudioDevice::name` / `unique_id`、`AudioDeviceList::enumerate*` に `# Errors` が無い。`AudioDevice::name` / `unique_id` は FFI バックエンドで `Error::NullPointer` を返し得る。`AudioDeviceList::enumerate*` は `Error::DeviceAccessDenied` / `Error::ComInitFailed` を返し得る。
- `src/capture.rs` / `src/playback.rs` の `AudioCapture::new*` / `AudioPlayback::new*` に `# Errors` が無い。`Error::InvalidDeviceId` / `Error::SessionCreateFailed` / `Error::InvalidAudioFormat` / `Error::DeviceNotFound` / `Error::DeviceAccessDenied` / `Error::ComInitFailed` を返し得る。`InvalidAudioFormat` は WASAPI 固有だが、`SessionCreateFailed` は全バックエンドで返り得る。どちらも `AudioCapture::new*` / `AudioPlayback::new*` の rustdoc には無く、README のキャプチャ節にしか書かれていない。
- `src/common.rs` の `AudioFrame::as_s16` / `as_f32` と `AudioFrameOwned::as_s16` / `as_f32` の `None` 条件 (フォーマット不一致 / `frames` または `channels` が 0 以下 / 長さ不足 / アライメント不一致 / フレーム数×チャンネル数×サンプルサイズが `usize` で表現できない場合) が doc に無い。
- `device_id` / `unique_id` の契約が不足している。`AudioDevice::unique_id()` の戻り値を `device_id` に渡すこと、同じ方向のデバイスを指定すること、CoreAudio では入出力の 2 エントリが同じ UID を持つこと、WASAPI では `unique_id()` が空文字列を返し得ることが書かれていない。
- `AudioCaptureSession::stop` / `AudioPlaybackSession::stop` がバックエンドの停止エラーを報告しないこと、`AudioFrame.data` がコールバック実行中だけ有効であること、`PlaybackFrame` の `data` が little-endian であることが書かれていない。`AudioCaptureSession` / `AudioPlaybackSession` を別スレッドへ move した場合に move 先で `initialize_thread()` が必要であることも書かれていない (0070 の範囲)。
- `AudioPlayback::config` がユーザー指定値をそのまま返すこと、実際のサンプルレート・チャンネル数は `sample_rate()` / `channels()` で取得することが書かれていない。`AudioPlayback::new` の doc にはデフォルト値 (再生は 48000 Hz / 2 ch) が無く、README の再生節にも記載が無い。`AudioPlaybackConfig` の doc には 0 以下の値の扱いがある。
- `AudioDeviceList::enumerate*` と `AudioCapture::start` / `AudioPlayback::start` はバックエンド API の応答待ちでブロックし得るが、rustdoc に記載がない。PulseAudio の start は C ヘッダに「サーバが応答しない場合、無期限にブロックする」とある。PipeWire の start は最大 10 秒待つが、`STREAM_STATE_TIMEOUT_SEC` のコメントにあるとおり厳密な上限ではない。列挙のブロックは C ヘッダにも記載がない。
- `unique_id` の安定性 (再起動・再接続をまたぐ同一性) は、バックエンドの API が保証する範囲を超えて保証されていない。`unique_id` は列挙時の識別子であり、再起動・再接続をまたいで同じ値を返すことをこのライブラリは保証しない。
- `AudioDeviceList::enumerate*` / `AudioCapture::new*` / `AudioPlayback::new*` / `AudioCapture::start` / `AudioPlayback::start` は Windows で `initialize_thread()` を呼んだスレッドで使う必要があるが、各 API の rustdoc にその要件が無い (README の例と `initialize_thread` の doc にはある)。

## 関連 issue

- コールバック panic 時の契約: `issues/0067-change-callback-panic-contract.md`
- `ThreadGuard` と move 先スレッドの初期化要件: `issues/0070-update-thread-guard-contract.md`
- `AudioDeviceList` のデフォルトデバイス API: `issues/0071-add-default-device-api.md`
- README / docs / `CHANGES.md` の整合: `issues/0073-doc-repository-documentation-consistency.md`
- デバイス情報の意味と既定値: `issues/0082-update-device-info-contract.md`
- 存在しない `device_id` のエラー統一: `issues/0084-change-device-id-detection-consistency.md`

## 設計方針

- 公開 API のうち `Result` を返す `AudioDevice::name` / `unique_id`、`AudioDeviceList::enumerate*`、`AudioCapture::new*` / `AudioPlayback::new*` に `# Errors` を追加し、各バックエンドで返り得る `Error` を列挙する。`AudioCapture::start` / `AudioPlayback::start`、`initialize_thread`、`PlaybackFrame::from_s16` / `from_f32` は既に `# Errors` / `# エラー` があるため、内容が実装と一致するか確認する。存在しない `device_id` のエラーは 0084 で `Error::DeviceNotFound` に統一される予定なので、0084 の完了後の挙動に合わせる。
- `AudioFrame::as_s16` / `as_f32` と `AudioFrameOwned::as_s16` / `as_f32` に `# 戻り値` 節を追加し、`None` を返す条件 (フォーマット不一致 / `frames` または `channels` が 0 以下 / 長さ不足 / アライメント不一致 / 計算のオーバーフロー) を書く。
- `device_id` は `AudioDevice::unique_id()` が返す値を受け取り、同じ方向のデバイスを指定する。`unique_id` はバックエンド固有の識別子で、空文字列を返す場合がある。CoreAudio では入出力の 2 エントリが同じ UID を持つため、リスト内で一意ではない。再起動・再接続をまたぐ同一性はバックエンドの保証範囲を超えて保証しない。
- セッション型の doc に、`AudioCaptureSession::stop` / `AudioPlaybackSession::stop` がバックエンドの停止エラーを報告しないこと、`stop` / Drop が実行中のコールバックの完了を待つこと (コールバックが戻らない限り戻らないこと) を書く。コールバック panic と move 先スレッドの初期化要件は 0067 / 0070 の範囲とし、本 issue では重複して詳細を書かず、既に書かれていない場合だけ最小限の記述を追加する。
- コールバックに渡される `AudioFrame` の `data` はコールバック実行中だけ有効であること、`PlaybackFrame` の `data` は little-endian の PCM データであることを書く。`AudioFrameOwned` は所有データなので寿命の制約はない。
- `AudioPlayback::config` はユーザー指定値をそのまま返し、実際のサンプルレート・チャンネル数は `sample_rate()` / `channels()` で取得することを書く。`AudioPlayback::new` のデフォルト値は `AudioPlaybackConfig` の doc を正本とし、`AudioPlayback::new` から参照する。
- `AudioDeviceList::enumerate*` と `AudioCapture::start` / `AudioPlayback::start` のブロック条件を、バックエンドごとに rustdoc に書く。PulseAudio の start は無期限にブロックし得る。PipeWire の start は最大 10 秒待つが厳密な上限ではない。列挙のブロックは PulseAudio / PipeWire で特に注意が必要である。
- `AudioDeviceList::enumerate*` / `AudioCapture::new*` / `AudioPlayback::new*` / `AudioCapture::start` / `AudioPlayback::start` の rustdoc に、Windows で `initialize_thread()` を呼んだスレッドで使う必要があることを書く。
- README と rustdoc の二重管理を避けるため、詳細は rustdoc を正本にする。README の使い方の節は rustdoc への参照と最小限の例に留める。README / `CHANGES.md` の整合は 0073 と重複するため、本 issue では rustdoc の契約を優先する。

## 完了条件

- 公開 API の `Result` / `Option` / panic / ブロック / スレッド要件が rustdoc から読み取れる。panic と move 先スレッドの初期化要件は 0067 / 0070 の範囲を含む。
- `AudioDevice::name` / `unique_id`、`AudioDeviceList::enumerate*`、`AudioCapture::new*` / `AudioPlayback::new*` の `# Errors` に、各バックエンドで返り得る `Error` が列挙されている。
- `AudioFrame` / `AudioFrameOwned` の `as_s16` / `as_f32` の `None` 条件が rustdoc に書かれている。
- `device_id` / `unique_id` の契約 (同じ方向、空文字列、CoreAudio の重複、安定性の限界) が rustdoc に書かれている。
- `AudioCaptureSession::stop` / `AudioPlaybackSession::stop` が停止エラーを報告しないこと、`AudioFrame.data` の寿命、`PlaybackFrame.data` の endian が rustdoc に書かれている。
- `AudioPlayback::config` が要求値をそのまま返し、実際の値は `sample_rate()` / `channels()` で取得できることが rustdoc に書かれている。
- `AudioDeviceList::enumerate*` / `AudioCapture::start` / `AudioPlayback::start` のブロック条件がバックエンドごとに rustdoc に書かれている。
- Windows で `initialize_thread()` が必要な API の rustdoc にその要件が書かれている。
- README と rustdoc の記述が矛盾しない。
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る。
