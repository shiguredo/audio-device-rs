# device_ffi.rs の負の count 未検証

- Priority: High
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-device-ffi-negative-count
- Polished: 2026-07-21

## 目的

`src/device_ffi.rs` の `FfiDeviceListImpl::enumerate` で、C 側から返された `count` の負値を検証していない問題を修正する。

## 優先度根拠

`count < 0` の場合 `count as usize` が巨大な値にラップし、確保されていないメモリを読みに行く（UB）。FFI 境界では C 側の戻り値を一切信頼してはならない。

## 現状

src/device_ffi.rs:93-100:

```rust
let ret = unsafe { (ops.enumerate_devices)(&mut devices_ptr, &mut count) };
if ret < 0 || devices_ptr.is_null() {
    return Err(Error::DeviceAccessDenied);
}
// 注: 以下は issue 著者の注釈。実コードにこのコメントは存在しない
// count < 0 のチェックがない
let devices: Vec<FfiDeviceImpl> = (0..count as usize) // 負なら巨大な範囲にラップ
    .filter_map(|i| {
        let device_ptr = unsafe { *devices_ptr.add(i) }; // 範囲外読み出し（UB）
```

### メモリリークパス

`ret >= 0` かつ `devices_ptr != null` かつ `count < 0` の場合、C 側は `devices_ptr` のメモリを確保済みである。このパスで早期 return すると `FfiDeviceListImpl` が生成されず `Drop`（147-153 行目）が動作しないため、C 側のメモリがリークする。修正時にはこのパスで `free_devices` を呼ぶ必要がある。

### 再現条件

C 側の `enumerate_devices` 実装がバグで負の `count` を返した場合に発生する。現状の C 実装（`audio_coreaudio.m` / `audio_pulse.c` / `audio_pipewire.c`）は負の `count` を返さないが、FFI 境界では C 側の戻り値を一切信頼しない方針で検証する。

## 設計方針

FFI 境界の防御的検証として、`count < 0` を C 側のプロトコル違反とみなしエラーを返す。エラーバリアントは既存の `Error::DeviceAccessDenied` を再利用する（C 側のデバイス列挙失敗という意味で同一カテゴリ。新バリアントの追加は `Error` enum の `match` を網羅している下流コードを破壊するため避ける）。

## 完了条件

- `count < 0` の場合に `free_devices` で C 側のメモリを解放してからエラーを返す
- `cargo clippy` / `cargo test` が通る
- 負の count を再現する単体テストは、モック禁止の規約（AGENTS.md）により FFI 関数ポインタを差し替えてテストすることができないため、コードレビューで担保する

## 解決方法

`device_ffi.rs:94` の条件を以下に変更する:

```rust
if ret < 0 || devices_ptr.is_null() || count < 0 {
    // count < 0 かつ devices_ptr が非 null の場合、C 側のメモリを解放する
    if !devices_ptr.is_null() && count < 0 {
        unsafe { (ops.free_devices)(devices_ptr, count) };
    }
    return Err(Error::DeviceAccessDenied);
}
```

注: `free_devices` に負の `count` を渡すことの安全性は C 側の実装に依存する。C 側の `free_devices`（`audio_coreaudio_free_devices` / `audio_pulse_free_devices` / `audio_pipewire_free_devices`）は `count <= 0` の場合にループが実行されないため安全。配列ポインタ自体は `free(devices)` で解放されるが、個別の `AudioDevice` 構造体はループが回らないためリークしうる。ただしこれは C 側がプロトコル違反（負の count）を犯したシナリオであり、回避不能なので許容する。

## 後方互換

公開 API の変更なし。`Error::DeviceAccessDenied` の再利用であり、`Error` enum に新規バリアントは追加しない
