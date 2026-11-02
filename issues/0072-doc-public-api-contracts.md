# 公開 API の契約を rustdoc に明記する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-public-api-docs
- Polished: {YYYY-MM-DD}

## 目的

公開 API を rustdoc だけで正しく使えるように、エラー・`None`・スレッド要件・ブロック条件などの契約を明記する。

## 現状

- `AudioDevice::name` / `unique_id`、`AudioDeviceList::enumerate*`、`AudioCapture::new*` / `AudioPlayback::new*` に `# Errors` が無い。WASAPI 固有の `InvalidAudioFormat` / `SessionCreateFailed` は README にしか書かれていない。
- `AudioFrame::as_s16` / `as_f32` の `None` 条件 (フォーマット不一致 / `frames` または `channels` が 0 以下 / 長さ不足 / アライメント不一致) が doc に無い。
- `device_id` / `unique_id` の契約が不足している。`AudioDevice::unique_id()` の戻り値を `device_id` に渡すこと、同じ方向のデバイスを指定すること、CoreAudio では入出力の 2 エントリが同じ UID を持つことが書かれていない。
- `AudioCaptureSession::stop` / `AudioPlaybackSession::stop` がバックエンドの停止エラーを報告しないこと、`AudioFrame.data` がコールバック実行中だけ有効であること、`PlaybackFrame` の `data` が little-endian であることが書かれていない。
- `AudioPlayback::config` がユーザー指定値をそのまま返すこと、`AudioPlayback::new` のデフォルト値 (再生は 48000 Hz / 2 ch) が README に無い。
- `AudioDeviceList::enumerate*` と `AudioCapture::start` / `AudioPlayback::start` が無期限にブロックし得ることが rustdoc に無い (C ヘッダにしか無い)。
- `unique_id` の安定性 (再起動・再接続をまたぐ一意性) が保証されない可能性がある。

## 設計方針

- 各 `Result` 返却 API に `# Errors` を追加し、考えられる `Error` を列挙する。
- `as_s16` / `as_f32` に `# 戻り値` 節を追加して `None` の 4 条件を書く。
- `device_id` / `unique_id` の意味、方向、重複、安定性の限界を書く。
- セッション型の doc に panic・ブロック・停止エラー非報告・move 先スレッドの初期化要件を書く。
- `AudioFrame` / `PlaybackFrame` のデータ寿命と endian を書く。
- README と rustdoc の二重管理を避けるため、詳細は rustdoc を正本にする。

## 完了条件

- 公開 API の `Result` / `Option` / panic / ブロック / スレッド要件が rustdoc から読み取れる。
- README と rustdoc の記述が矛盾しない。
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る。
