# デフォルトデバイスを取得する API を追加する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/add-default-device-api
- Polished: {YYYY-MM-DD}

## 目的

`device_id: None` が指すデフォルトデバイスを利用者が確認できるようにする。

## 現状

- `AudioCaptureConfig` / `AudioPlaybackConfig` の `device_id: None` は「デフォルトデバイス」と定義されているが、どのデバイスがデフォルトかを取得する公開 API が無い。
- `AudioDeviceList` にも `AudioDevice` にも `is_default` が無く、列挙結果からデフォルトを判別できない。
- バックエンドはデフォルト取得手段を持つ (WASAPI の `GetDefaultAudioEndpoint`、CoreAudio のデフォルトデバイスプロパティ、PulseAudio の server info、PipeWire の metadata)。WASAPI の `get_device_by_id` は内部で `GetDefaultAudioEndpoint` を使っているが公開されていない。

## 設計方針

- `AudioDeviceList::default_input()` / `default_output()`、または `AudioDevice::is_default()` を追加する。
- バックエンドごとにデフォルトデバイスを取得する FFI / Win32 API を実装する。
- 取得できない場合は `Option` または `Result` で表現し、デバイス 0 件時と区別する。
- API 追加として `CHANGES.md` に `[ADD]` で記載する。

## 完了条件

- デフォルトの入力・出力デバイスを取得できる。
- `device_id: None` で使われるデバイスと API が返すデバイスが一致する。
- テストまたは examples で動作を確認できる。
- `cargo test --workspace` が通る。
