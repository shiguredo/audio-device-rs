# device_ffi.rs の負の count 未検証

- Priority: High
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-device-ffi-negative-count
- Polished: {YYYY-MM-DD}

## 目的

`src/device_ffi.rs` の `FfiDeviceListImpl::enumerate` で、C 側から返された `count` の負値を検証していない問題を修正する。

## 優先度根拠

`count < 0` の場合 `count as usize` が巨大な値にラップし、確保されていないメモリを読みに行く（UB）。FFI 境界では C 側の戻り値を一切信頼してはならない。

## 現状

src/device_ffi.rs:96-100:

```rust
let ret = unsafe { (ops.enumerate_devices)(&mut devices_ptr, &mut count) };
if ret < 0 || devices_ptr.is_null() {
    return Err(Error::DeviceAccessDenied);
}
// count < 0 のチェックがない
let devices: Vec<FfiDeviceImpl> = (0..count as usize) // 負なら巨大な範囲
    .filter_map(|i| {
        let device_ptr = unsafe { *devices_ptr.add(i) }; // 範囲外読み出し
```

## 完了条件

- `count < 0` の場合にエラーを返す
- `cargo clippy` / `cargo test` が通る

## 解決方法

条件を `ret < 0 || devices_ptr.is_null() || count < 0` に追加する。
