# 存在しない device_id の検出を早め、エラー種別をバックエンド間で統一する

- Created: 2026-11-03
- Completed: {YYYY-MM-DD}
- Branch: feature/change-device-id-detection
- Polished: {YYYY-MM-DD}

## 目的

存在しない `device_id` を指定した場合の検出時期と返るエラーがバックエンドで異なるため、利用側のエラーハンドリングがプラットフォームごとに変わってしまう。検出を早め、`Error::DeviceNotFound` に揃える。

## 現状

- CoreAudio は `audio_coreaudio_session_create` / `audio_coreaudio_playback_session_create` の `find_device_by_uid` で存在しない ID を検出し、`NULL` を返す。Rust 側の `FfiCaptureImpl::new` / `FfiPlaybackImpl::new` は `Error::SessionCreateFailed` にする。
- PulseAudio は `audio_pulse_session_create` / `audio_pulse_playback_session_create` で `device_id` を保持するだけで検証しない。`audio_pulse_session_start` / `audio_pulse_playback_session_start` の `pa_stream_connect_record` / `pa_stream_connect_playback` または `wait_stream_ready` で失敗し、Rust 側は `Error::SessionStartFailed` にする。
- PipeWire は `audio_pipewire_session_create` / `audio_pipewire_playback_session_create` で `device_id` を保持するだけで検証しない。`audio_pipewire_session_start` / `audio_pipewire_playback_session_start` の `wait_for_stream_ready` が最大 10 秒 (`STREAM_STATE_TIMEOUT_SEC`) 待って失敗し、Rust 側は `Error::SessionStartFailed` にする。
- WASAPI は `get_device_by_id` が `WasapiCaptureImpl::new` / `WasapiPlaybackImpl::new` から呼ばれ、`GetDevice` の失敗を `Error::DeviceNotFound` にする。既に `new` で検出している。
- 同じ「存在しない `device_id`」でも `Error::SessionCreateFailed` / `Error::SessionStartFailed` / `Error::DeviceNotFound` とばらつき、PipeWire は最大 10 秒待つ。

## 設計方針

- 存在しない `device_id` は、バックエンドによらず `Error::DeviceNotFound` を返す。
- 検出は `new` または `start` のできるだけ早い段階で行い、PipeWire の 10 秒タイムアウトを待たない。
- C バックエンドが not-found を区別できない場合は、Rust 側の `new` でデバイス一覧と照合するなど、バックエンド共通の方法で判定する。C の戻り値に not-found を表す値を追加する方法でもよい。
- WASAPI は既に `new` で `Error::DeviceNotFound` を返すため、変更不要である。方向 (入力 / 出力) の検証は issue 0060 の範囲とする。
- `CHANGES.md` の `## develop` に `[CHANGE]` エントリを追加する。エラー種別が変わるため後方互換のない変更として扱う。
- `audio.h` の create / start の契約に、存在しない `device_id` の扱いを追記する。

## 完了条件

- 存在しない `device_id` を指定すると、CoreAudio / PulseAudio / PipeWire / WASAPI のすべてで `Error::DeviceNotFound` が返る。
- PipeWire で存在しない `device_id` を指定した場合、10 秒のタイムアウトを待たずに `Error::DeviceNotFound` が返る。
- 存在しない `device_id` を指定するテストが追加されている。
- `CHANGES.md` に `[CHANGE]` エントリが追加されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が CI の全構成で通る。
