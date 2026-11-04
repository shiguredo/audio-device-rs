# 存在しない device_id の検出を早め、エラー種別をバックエンド間で統一する

- Created: 2026-11-03
- Completed: {YYYY-MM-DD}
- Branch: feature/change-device-id-detection
- Polished: 2026-11-04

## 目的

存在しない `device_id` を指定した場合の検出時期と返るエラーがバックエンドで異なるため、利用側のエラーハンドリングがプラットフォームごとに変わってしまう。検出を早め、`Error::DeviceNotFound` に揃える。

## 現状

- CoreAudio は `audio_coreaudio_session_create` / `audio_coreaudio_playback_session_create` の `find_device_by_uid` で存在しない ID を検出し、`NULL` を返す。Rust 側の `FfiCaptureImpl::new` / `FfiPlaybackImpl::new` は `Error::SessionCreateFailed` にする。`find_device_by_uid` は issue 0080 で削除する予定であり、デバイスの存在確認は `AudioQueueSetProperty` の成否で行う予定である。
- PulseAudio は `audio_pulse_session_create` / `audio_pulse_playback_session_create` で `device_id` を保持するだけで検証しない。`audio_pulse_session_start` / `audio_pulse_playback_session_start` の `pa_stream_connect_record` / `pa_stream_connect_playback` または `wait_stream_ready` で失敗し、Rust 側は `Error::SessionStartFailed` にする。issue 0059 で start に約 10 秒のタイムアウトを追加する予定である。
- PipeWire は `audio_pipewire_session_create` / `audio_pipewire_playback_session_create` で `device_id` を保持するだけで検証しない。`audio_pipewire_session_start` / `audio_pipewire_playback_session_start` の `wait_for_stream_ready` が約 10 秒 (`STREAM_STATE_TIMEOUT_SEC`) 待って失敗し、Rust 側は `Error::SessionStartFailed` にする。`wait_for_stream_ready` のコメントにあるとおり、この値は厳密な上限ではない。
- WASAPI は `get_device_by_id` が `WasapiCaptureImpl::new` / `WasapiPlaybackImpl::new` から呼ばれ、`GetDevice` の失敗を `Error::DeviceNotFound` にする。既に `new` で検出している。
- 同じ「存在しない `device_id`」でも `Error::SessionCreateFailed` / `Error::SessionStartFailed` / `Error::DeviceNotFound` とばらつき、PipeWire は約 10 秒待つ。

## 関連 issue

- `issues/0080-remove-dead-code-and-unused-configs.md`: CoreAudio の `find_device_by_uid` を削除し、デバイスの存在確認を `AudioQueueSetProperty` の成否で行う。0080 が先に完了した場合は、本 issue の現状にある `find_device_by_uid` の記述を更新する。
- `issues/0060-bug-wasapi-device-and-session-correctness.md`: WASAPI の方向 (入力 / 出力) の検証を扱う。存在しない `device_id` の検出時期とエラー種別は本 issue の範囲である。
- `issues/0068-change-backend-error-mapping.md`: C バックエンドの start エラーコードの写像を扱う。存在しない `device_id` の `Error::DeviceNotFound` への写像は本 issue の範囲である。
- `issues/0072-doc-public-api-contracts.md`: `AudioCapture::new*` / `AudioPlayback::new*` の `# Errors` を整備し、本 issue の完了後の挙動に合わせる。
- `issues/0059-bug-pulseaudio-liveness.md`: PulseAudio の start に約 10 秒のタイムアウトを追加する。本 issue の早期検出はこのタイムアウトを待たない。

## 設計方針

- 存在しない `device_id` は、バックエンドによらず `Error::DeviceNotFound` を返す。
- Rust API では検出を `new` で行い、`start` では行わない。FFI バックエンドの `new*` は、`config.device_id` が `Some` の場合、対応する方向（キャプチャは入力、再生は出力）のデバイス一覧を取得して `unique_id` と照合する方式を基本とする。一致するデバイスがなければ `Error::DeviceNotFound` を返す。これにより PipeWire の `wait_for_stream_ready` の約 10 秒のタイムアウトを待たない。
- C バックエンドの create が not-found を区別できる場合は、その戻り値を使ってよい。C の create / start の戻り値に not-found を表す値を追加する場合は、`build.rs` の bindgen allowlist に追加する。
- デバイス一覧の取得自体が失敗した場合は、そのエラーを返し、`Error::DeviceNotFound` には丸めない。
- not-found 以外の create / start 失敗は、既存のエラー写像を維持する。NUL バイトを含む `device_id` は従来どおり `Error::InvalidDeviceId` を返す。`FfiCaptureImpl::new` / `FfiPlaybackImpl::new` の `session_create` 失敗は `Error::SessionCreateFailed`、`start` 失敗は既存の `Error::SessionStartFailed`（issue 0068 の完了後はその写像）とする。
- 対応する方向のデバイス一覧と照合するため、方向が一致しない `device_id` も `Error::DeviceNotFound` になる。WASAPI の方向検証は issue 0060 の範囲とする。
- WASAPI は `device_id` を指定した場合、既に `new` で `Error::DeviceNotFound` を返すため、変更不要である。
- `CHANGES.md` の `## develop` に `[CHANGE]` エントリを追加する。エラー種別が変わるため後方互換のない変更として扱う。エントリには、存在しない `device_id` を指定した場合に `Error::DeviceNotFound` を返すことと、PipeWire のタイムアウトを待たずに検出することを書く。
- `audio.h` の create / start の契約に、存在しない `device_id` を指定した場合に create または start のいずれかで失敗し、どちらで失敗するかはバックエンド固有であることを追記する。
- `tests/test_capture.rs` / `tests/test_playback.rs` に、存在しない `device_id` を指定して `Error::DeviceNotFound` を返すテストを追加する。macOS のキャプチャはマイク権限の影響を受けるため、権限がない環境ではキャプチャのテストをスキップし、再生のテストで確認する。

## 完了条件

- 存在しない `device_id` を指定すると、CoreAudio / PulseAudio / PipeWire / WASAPI のすべてで `Error::DeviceNotFound` が返る。検出は `new` で行われ、`start` を呼ぶ前に返る。
- PipeWire で存在しない `device_id` を指定した場合、`wait_for_stream_ready` の約 10 秒のタイムアウトを待たずに `Error::DeviceNotFound` が返る。
- 存在しない `device_id` 以外の create / start 失敗は、既存のエラー写像を維持している。
- デバイス一覧の取得に失敗した場合、`new` はそのエラーを返し、`Error::DeviceNotFound` にはしない。
- 存在しない `device_id` を指定するテストが追加されている。`AudioCapture::new` と `AudioPlayback::new` の両方を対象とする（macOS のキャプチャはマイク権限がない場合にスキップする）。
- `audio.h` の create / start の契約に、存在しない `device_id` の扱いが追記されている。
- `CHANGES.md` に `[CHANGE]` エントリが追加されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が CI の全構成で通る。
