# device_ffi のテストからモック・スタブを除去する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-remove-device-ffi-test-stubs
- Polished: {YYYY-MM-DD}

## 目的

`src/device_ffi.rs` の単体テストが `AGENTS.md` の「モックやスタブは絶対に利用しないこと」に違反している状態を解消する。

## 現状

- `src/device_ffi.rs` の `#[cfg(test)] mod tests` は、`OPS_EMPTY` と、`unreachable!` を本体とする 6 個の偽 FFI 関数 (`empty_enumerate_devices` / `unreachable_device_name` / `unreachable_device_unique_id` / `unreachable_device_channels` / `unreachable_device_sample_rate` / `unreachable_device_type`) を定義している。
- これらは C バックエンドの代役を務めるスタブであり、`AGENTS.md` のモック・スタブ禁止規約に反する。
- 空リストの扱い以外の `FfiDeviceListImpl::enumerate` の分岐 (負の `ret`、負の `count`、`devices_ptr` が NULL で `count > 0`、未知の device_type、NULL デバイスポインタ) も未検証である。

## 設計方針

- 「`ret` / `count` / `devices_ptr` の組み合わせをどう解釈するか」を純粋関数 (`fn classify_enumerate_result(ret: i32, count: i32, devices_ptr_is_null: bool) -> Result<()>` など) に切り出し、その単体テストに置き換える。
- 未知の device_type や NULL デバイスポインタの扱いも、純粋関数のテストで固定する。
- スタブを残す必要がある場合は、なぜ規約の例外になるのかをリポジトリの規約側で明文化する。

## 完了条件

- `src/device_ffi.rs` のテストから偽 FFI 関数のスタブが無くなる。
- 列挙結果の解釈が純粋関数としてテストされている。
- `cargo fmt` / `cargo clippy --workspace --all-targets` / `cargo test --workspace` が通る。
