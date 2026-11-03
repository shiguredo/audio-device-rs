# バックエンドのエラーコードを公開エラーへ写像する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/change-backend-error-mapping
- Polished: 2026-11-04

## 目的

C バックエンドが返すエラーコードの情報を公開 API から観測できるようにし、原因切り分けを可能にする。

## 現状

- `FfiCaptureImpl::start` / `FfiPlaybackImpl::start` は戻り値の負値をすべて `Error::SessionStartFailed` にしている。`audio_pipewire.h` の `AudioPipeWireResult` は 12 個の負値コードを定義し「負の値は AudioPipeWireResult に対応する」と契約しているが、Rust からは区別できない。
- `FfiDeviceListImpl::enumerate` は C 側の負の戻り値をすべて `Error::DeviceAccessDenied` にしている。PulseAudio のサーバ未起動 (-3) やメモリ確保失敗 (-2) が同じエラーになる。PipeWire の列挙は -1 (引数不正)、-2 (main loop / context の確保失敗)、-3 (context 接続失敗)、-4 (registry 取得失敗) を返し、-9 (タイムアウト) はセッション開始の `wait_for_stream_ready` だけが返す。
- `build.rs` の bindgen allowlist は変数として `AUDIO_FORMAT_.*` と `AUDIO_DEVICE_TYPE_.*` だけを許可しており、`AudioPipeWireResult` の列挙値は Rust のバインディングに含まれていない。PulseAudio / CoreAudio の C ヘッダにはエラーコード定義自体が無い。
- `Error::DeviceAccessDenied` の Display は "audio access denied" であり、メモリ確保失敗や列挙失敗にも使われるため原因と一致しないケースがある。
- `Error::ComInitFailed` の Display は "thread has incompatible apartment model" に固定されているが、`CO_E_NOTINITIALIZED` にも使われる。
- `FfiCaptureImpl::stop` / `FfiPlaybackImpl::stop` は C の `session_stop` の戻り値を捨てている。公開 API の `AudioCaptureSession::stop` / `AudioPlaybackSession::stop` は戻り値を返さない。

## 設計方針

- `Error` に `BackendUnavailable` / `Timeout` / `ResourceExhausted` を追加する。`Error` は公開 enum であり、variant 追加は後方互換のない変更として `CHANGES.md` の `## develop` に `[CHANGE]` で記載する。
- 対象は `FfiCaptureImpl::start` / `FfiPlaybackImpl::start` / `FfiDeviceListImpl::enumerate` の戻り値とする。`FfiCaptureImpl::new` / `FfiPlaybackImpl::new` の `SessionCreateFailed` と、`stop` の戻り値は本 issue では扱わない。
- `CaptureOps` / `PlaybackOps` / `DeviceOps` にバックエンドごとの写像関数を持たせ、`start` / `enumerate` から呼ぶ。C の戻り値の意味は各バックエンドのヘッダに定義する。
- `build.rs` の bindgen allowlist に `AudioPipeWireResult` と、追加する PulseAudio / CoreAudio のエラーコード enum を追加する。C 側の実装もヘッダで定義した enum の値を使う。
- `audio_pipewire.h` の `AudioPipeWireResult` を次のように写像する。
  - `AUDIO_PIPEWIRE_ERROR_CONTEXT_CONNECT` / `AUDIO_PIPEWIRE_ERROR_CORE` は `Error::BackendUnavailable`
  - `AUDIO_PIPEWIRE_ERROR_TIMEOUT` は `Error::Timeout`
  - `AUDIO_PIPEWIRE_ERROR_THREAD_LOOP_START` / `AUDIO_PIPEWIRE_ERROR_PROPERTIES` / `AUDIO_PIPEWIRE_ERROR_STREAM_CREATE` は `Error::ResourceExhausted`
  - それ以外の負値は `Error::SessionStartFailed`
- PipeWire の列挙は -2 を `Error::ResourceExhausted`、-3 を `Error::BackendUnavailable`、-4 と -1 を `Error::DeviceAccessDenied` に写像する。issue 0058 で追加される列挙タイムアウトは `Error::Timeout`、core エラーは `Error::BackendUnavailable` に写像する。
- `audio_pulse.h` と `audio_coreaudio.h` に列挙・セッション開始それぞれの戻り値の enum を追加する。PulseAudio の列挙は -2 を `Error::ResourceExhausted`、-3 を `Error::BackendUnavailable`、-4 と -1 を `Error::DeviceAccessDenied` に写像する。PulseAudio の start は -2 を `Error::ResourceExhausted`、-3 を `Error::BackendUnavailable`、-1 / -4 / -5 を `Error::SessionStartFailed` に写像する。CoreAudio は列挙の -2 を `Error::ResourceExhausted`、-1 を `Error::DeviceAccessDenied`、start の -1 を `Error::SessionStartFailed` に写像する。
- 権限不足は C 側が区別しないため、列挙では既存の `Error::DeviceAccessDenied` で表し、start では `Error::SessionStartFailed` にフォールバックする。CoreAudio の start は権限不足とリソース不足を区別できないため、`Error::SessionStartFailed` にフォールバックする。
- `FfiDeviceListImpl::enumerate` の `count < 0` と `count > 0` かつ `devices_ptr` が NULL の場合は、従来どおり `Error::DeviceAccessDenied` にする。
- C 側が新しいコードを追加した場合に備え、未知の負値は `Error::SessionStartFailed` / `Error::DeviceAccessDenied` にフォールバックする。
- PulseAudio のタイムアウト (issue 0059) は `Error::Timeout` に写像する。タイムアウトを他の失敗と区別できるよう、`audio_pulse.h` にタイムアウト専用の戻り値を定義する。存在しない `device_id` の `Error::DeviceNotFound` への写像は issue 0084 の範囲とし、本 issue では扱わない。
- `Error::DeviceAccessDenied` の Display を "failed to access audio device" に変更し、アクセス拒否以外の列挙失敗にも使える表現にする。`DeviceAccessDenied` の doc コメントも、アクセス拒否に限定しない説明に更新する。`ComInitFailed` の Display を "COM initialization failed" に変更し、HRESULT に依存しない表現にする。
- `audio.h` の共通仕様の「負の値の意味はバックエンド固有」という記述は維持し、各バックエンドのヘッダに定義したエラーコードと Rust の写像が一致することを明記する。
- `AudioCapture::start` / `AudioPlayback::start` の rustdoc の `# Errors` に新しい variant を追加する。列挙 API の `# Errors` は issue 0072 で整備する。

## 完了条件

- サーバ未起動、タイムアウト、リソース不足が、C 側が区別できる範囲で区別できる。権限不足は C 側が区別しないため、列挙では既存の `Error::DeviceAccessDenied`、start では `Error::SessionStartFailed` で表す。
- `audio_pipewire.h` / `audio_pulse.h` / `audio_coreaudio.h` のエラーコード定義と Rust の変換が一致しており、`audio.h` の共通仕様に、負の値がバックエンド固有であり各バックエンドのヘッダで定義されることが記載されている。
- `FfiCaptureImpl::start` / `FfiPlaybackImpl::start` / `FfiDeviceListImpl::enumerate` の各戻り値と未知の負値の写像を単体テストで確認している。
- `Error::DeviceAccessDenied` と `Error::ComInitFailed` の Display が原因に依存しない表現になっており、`DeviceAccessDenied` の doc コメントもアクセス拒否に限定しない説明になっている。
- `AudioCapture::start` / `AudioPlayback::start` の rustdoc の `# Errors` に新しい variant が記載されている。
- `pbt/tests/prop_error.rs` の `ERROR_SAMPLERS` に新しい variant を追加し、全 variant の Display を検証している。
- `CHANGES.md` の `## develop` に `[CHANGE]` エントリが追加されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が通り、CI の PulseAudio / PipeWire / CoreAudio / WASAPI 構成が成功する。
