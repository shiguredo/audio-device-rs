# Windows のデバイス列挙で GetId と PROPVARIANT を解放する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-windows-device-enum-memory-leaks
- Polished: {YYYY-MM-DD}

## 目的

Windows のデバイス列挙が呼び出しごとにメモリをリークする問題を修正する。

## 現状

- `device_wasapi.rs` の `enumerate_devices_by_type` は `IMMDevice::GetId` の戻り値 `PWSTR` を `to_string()` で読むだけで、`CoTaskMemFree` を呼んでいない。Win32 API の契約では `GetId` の戻り値は呼び出し側が `CoTaskMemFree` で解放する。
- 同じ関数の `get_device_name` は `IPropertyStore::GetValue` の戻り値 `PROPVARIANT` を読むだけで、`PropVariantClear` を呼んでいない。`PKEY_Device_FriendlyName` は `VT_LPWSTR` を返すため、`CoTaskMemAlloc` された文字列が解放されない。
- `windows` クレート 0.62.2 の `PWSTR` と `PROPVARIANT` は Drop を持たない。同じモジュールの `get_device_format` は `CoTaskMemFree` を呼んでおり、`GetId` と `GetValue` だけが取りこぼされている。
- `AudioDeviceList::enumerate()` を繰り返すアプリケーションでリークが累積する。

## 設計方針

- `GetId` の戻り値は `String` へコピーした直後に `CoTaskMemFree(Some(id.0 as *const _))` を呼ぶ。`to_string()` の失敗を `unwrap_or_default()` で潰さず、デバイスをスキップするなど明示的に扱う。
- `get_device_name` は `PROPVARIANT` を `let mut` で受け、文字列を `String` にコピーした直後に `PropVariantClear(&mut pv)` を呼ぶ。`VT_LPWSTR` 以外の型や null ポインタの経路でも必ず解放する。

## 完了条件

- `GetId` と `GetValue` の戻り値がすべての経路で解放される。
- 列挙を繰り返してもメモリ使用量が増えないことをコードレビューで確認できる。
- Windows 実機の device-test でデバイス列挙が成功する。
- `cargo clippy --target x86_64-pc-windows-msvc --workspace -- -D warnings` が通る。
