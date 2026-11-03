# Windows のデバイス列挙で GetId と PROPVARIANT を解放する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-windows-device-enum-memory-leaks
- Polished: 2026-11-03

## 目的

Windows のデバイス列挙が呼び出しごとにメモリをリークする問題を修正する。

## 現状

- `device_wasapi.rs` の `enumerate_devices_by_type` は `IMMDevice::GetId` の戻り値 `PWSTR` を
  `to_string()` で読むだけで、`CoTaskMemFree` を呼んでいない。Microsoft Learn の
  [`IMMDevice::GetId`](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdevice-getid)
  では、戻り値の文字列は呼び出し側が `CoTaskMemFree` で解放する契約になっている。
- `enumerate_devices_by_type` から呼ばれる `get_device_name` は `IPropertyStore::GetValue` の
  戻り値 `PROPVARIANT` を読むだけで、`PropVariantClear` を呼んでいない。
  `PKEY_Device_FriendlyName` は `VT_LPWSTR` 型のプロパティであり、値が存在する場合は
  `CoTaskMemAlloc` された文字列が返るが、解放されない。Microsoft Learn の
  [`Device Properties`](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-properties)
  の例でも `GetValue` の後に `PropVariantClear` を呼んでいる。
- `windows` クレート 0.62.2 の `PWSTR` と `PROPVARIANT` は Drop を持たない。同じモジュールの
  `get_device_format` は `CoTaskMemFree` を呼んでおり、`GetId` と `GetValue` だけが
  取りこぼされている。
- `AudioDeviceList::enumerate()` を繰り返すアプリケーションでリークが累積する。

## 設計方針

- `GetId` の戻り値は、`to_string()` の成功・失敗にかかわらず、`to_string()` の呼び出し
  直後に `CoTaskMemFree(Some(id.0 as *const _))` を呼ぶ。`to_string()` が失敗した場合は、
  解放後にデバイスをスキップする。`to_string()` の失敗を `unwrap_or_default()` で潰さない。
- `get_device_name` は `PROPVARIANT` を `Ok(mut pv)` で受け、文字列を `String` にコピーした
  直後に `let _ = PropVariantClear(&mut pv);` を呼ぶ。`String::from_utf16` の成功・失敗に
  かかわらず `PropVariantClear` を呼ぶ。`VT_LPWSTR` 以外の型や null ポインタの経路でも
  必ず解放する。戻り値は `let _ =` で破棄する。
- `PropVariantClear` は
  `windows::Win32::System::Com::StructuredStorage::PropVariantClear` を使うため、`use` に
  追加する。

## 完了条件

- `GetId` と `GetValue` の戻り値がすべての経路で解放される。
- 列挙を繰り返してもメモリ使用量が増えないことをコードレビューで確認できる。
- Windows 実機の device-test でデバイス列挙が成功する。
- Windows で `cargo fmt --all -- --check` と
  `cargo clippy --target x86_64-pc-windows-msvc --workspace -- -D warnings` が通る。
