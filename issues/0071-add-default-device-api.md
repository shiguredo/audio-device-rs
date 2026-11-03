# デフォルトデバイスを取得する API を追加する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/add-default-device-api
- Polished: 2026-11-04

## 目的

`device_id: None` が指すデフォルトデバイスを利用者が確認できるようにする。

## 現状

- `AudioCaptureConfig` / `AudioPlaybackConfig` の `device_id: None` は「デフォルトデバイス」と定義されているが、どのデバイスがデフォルトかを取得する公開 API が無い。
- `AudioDeviceList` にも `AudioDevice` にも、デフォルトを判別する API が無い。列挙結果の順序はデフォルトを表さないため、列挙結果からデフォルトを判別できない。
- 各バックエンドのライブラリにはデフォルト取得 API があるが、現在の C / Win32 実装は公開していない。
  - WASAPI: `IMMDeviceEnumerator::GetDefaultAudioEndpoint`。`src/device_wasapi.rs` の `get_device_by_id` は `device_id: None` のときに `GetDefaultAudioEndpoint(eCapture/eRender, eConsole)` を使うが、`pub(crate)` で公開 API ではない。
  - CoreAudio: `kAudioHardwarePropertyDefaultInputDevice` / `kAudioHardwarePropertyDefaultOutputDevice`。
  - PulseAudio: `pa_context_get_server_info` の `default_source_name` / `default_sink_name`。
  - PipeWire: metadata の `default.audio.source` / `default.audio.sink`。
- 現在の `src/audio_coreaudio.m` / `src/audio_pulse.c` / `src/audio_pipewire.c` は、セッション作成時に `device_id: NULL` を各バックエンドのデフォルト選択に委ねているだけで、デフォルトの `unique_id` を取得して返す関数を持たない。

## 設計方針

- デフォルトデバイスを直接取得できる API を優先するため、`AudioDeviceList` に次の 2 メソッドを追加する。この issue では `AudioDevice::is_default()` は追加しない。
  - `pub fn default_input(&self) -> Result<Option<&AudioDevice>>`
  - `pub fn default_output(&self) -> Result<Option<&AudioDevice>>`
- 戻り値の契約:
  - `Ok(Some(device))`: このリストに含まれるデバイスの中に、バックエンドのデフォルト入力 / 出力デバイスがある。
  - `Ok(None)`: リストが空、またはリスト内にデフォルトデバイスが無い。リストが空の場合はバックエンドへ問い合わせず `Ok(None)` を返す。デバイス 0 件かどうかは `AudioDeviceList::is_empty()` で判別できる。
  - `Err`: バックエンドのデフォルト取得に失敗した。既存の `Error` の範囲で、列挙時と同じ方針で写像する。
- バックエンドが「デフォルトデバイスが無い」を返した場合は `Ok(None)` とし、取得処理自体の失敗だけを `Err` とする。
- リスト内のデバイスの `unique_id()` が取得できない場合は、そのデバイスを照合対象から除外する。
- デフォルトの取得は、`device_id: None` のセッション作成で各バックエンドが選ぶデフォルトデバイスと同じデフォルトデバイスを返す API を使う。これにより、`device_id: None` で使われるデバイスと API が返すデバイスが一致することをコード上で保証する。
- 各バックエンドに、指定した方向のデフォルトデバイスの `unique_id` を返す FFI 関数または Win32 API の呼び出しを追加する。C 側で返した文字列の解放方法を各ヘッダに明記する。
  - `src/device_ffi.rs` の `DeviceOps` にデフォルト `unique_id` 取得関数を追加し、`src/audio_coreaudio.m` / `src/audio_pulse.c` / `src/audio_pipewire.c` と各ヘッダに実装を追加する。
  - WASAPI は `src/device_wasapi.rs` の `get_device_by_id(None, device_type)` を再利用し、`IMMDevice::GetId()` で `unique_id` を取得する。
- `AudioDeviceList::default_input()` / `default_output()` は、`AudioDeviceListInner` が保持するバックエンドと同じバックエンドへ問い合わせ、`self.as_slice()` のうち `device_type()` が要求した方向と一致するデバイスを対象に、`unique_id()` を照合する。`default-*` feature に依存せず、`AudioDeviceList` が利用できるすべての feature 構成で使える。
- 新しい公開 API には rustdoc を付け、`Ok(Some)` / `Ok(None)` / `Err` の条件、リストに含まれるデバイスだけを返すこと、`device_id: None` との一致、`initialize_thread()` の要件が既存の列挙 API と同じであることを書く。
- `CHANGES.md` の `## develop` に `[ADD]` エントリを追加する。
- `tests/test_device.rs` に、デフォルトバックエンドの `AudioDeviceList::enumerate()` を使うテストを追加する。`initialize_thread()` を呼んでから列挙する。デバイスが 0 件の場合は、`default_input()` / `default_output()` が `Ok(None)` を返すことを確認する。入力デバイスが 1 件以上ある場合は `default_input()` が入力デバイスを返し、`unique_id()` が空でないことと、`device_id: None` の `AudioCapture::new` が成功することを確認する。出力デバイスが 1 件以上ある場合は `default_output()` が出力デバイスを返し、`unique_id()` が空でないことと、`device_id: None` の `AudioPlayback::new` が成功することを確認する。方向ごとのデバイスが無い場合は、その方向の確認をスキップする。マイク権限などの環境依存の理由で `AudioCapture::new` が失敗する場合は、入力方向の `new` の確認だけをスキップする。

## 完了条件

- デフォルトの入力・出力デバイスを取得できる。
- `device_id: None` で使われるデバイスと API が返すデバイスが一致する（同じバックエンドのデフォルトデバイス選択に基づく）。
- テストで動作を確認できる。デバイスが 0 件の場合は `Ok(None)` を確認し、方向ごとのデバイスが無い場合はその方向をスキップする。
- 新しい公開 API の rustdoc が追加されている。
- `CHANGES.md` に `[ADD]` エントリが追加されている。
- `default-*` を有効にしない明示バックエンド構成でも、`AudioDeviceList` と新しい API がコンパイルできる。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が通る。
