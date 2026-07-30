# device_ffi.rs デバイス 0 件の列挙が DeviceAccessDenied エラーになる問題を修正する

- Created: 2026-07-30
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-device-ffi-empty-list
- Polished: {YYYY-MM-DD}

## 目的

オーディオデバイスが存在しない環境（ヘッドレスサーバ、CI コンテナ等）で `AudioDeviceList::enumerate()` が誤って `DeviceAccessDenied` エラーを返す問題を修正する。

## 現状

`src/device_ffi.rs` の `FfiDeviceListImpl::enumerate` 関数で、C 側から返された `devices_ptr` が NULL の場合に一律でエラーを返している:

```rust
let ret = unsafe { (ops.enumerate_devices)(&mut devices_ptr, &mut count) };
if ret < 0 || devices_ptr.is_null() {
    return Err(Error::DeviceAccessDenied);
}
```

しかし 3 バックエンドすべて、デバイス 0 件の場合に `devices_ptr = NULL`、`count = 0`、`return 0`（成功）を返す:

- `audio_coreaudio.m` の `audio_coreaudio_enumerate_devices`: `deviceCount == 0` 時に `*devices = NULL; *count = 0; return 0`
- `audio_pulse.c` の `audio_pulse_enumerate_devices`: 列挙結果 0 件時に `*devices = NULL`（`enum_ctx.devices` の初期値）
- `audio_pipewire.c` の `audio_pipewire_enumerate_devices`: 同上

つまり legitimately 空のデバイスリストがエラーとして扱われる。

## 設計方針

`ret < 0` の場合のみエラーとし、`ret == 0` で `count == 0`（または `devices_ptr == NULL`）の場合は空のデバイスリストとして正常に返す。

## 完了条件

デバイスが存在しない環境で `AudioDeviceList::enumerate()` が空リスト（`len() == 0`）を返すこと。`DeviceAccessDenied` エラーは実際のアクセス拒否時のみ返すこと。

## 解決方法

`src/device_ffi.rs` の `FfiDeviceListImpl::enumerate` 関数内のエラーチェックを修正する:

1. `ret < 0` の場合のみ `Err(Error::DeviceAccessDenied)` を返す
2. `ret >= 0` かつ `devices_ptr.is_null()` または `count <= 0` の場合は、空の `devices` Vec を持つ正常な `Self` を返す
