# device_ffi のテストからモック・スタブを除去する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-remove-device-ffi-test-stubs
- Polished: 2026-11-04

## 目的

`src/device_ffi.rs` の単体テストが `AGENTS.md` の「モックやスタブは絶対に利用しないこと」に違反している状態を解消する。

## 現状

- `src/device_ffi.rs` の `#[cfg(test)] mod tests` は、`OPS_EMPTY` と次の 7 個の偽 FFI 関数を定義している。
  - `empty_enumerate_devices`: `*devices_ptr` を NULL にし、`*count` を 0 にして `0` を返す。
  - `empty_free_devices`: 何もしない。
  - `unreachable_device_name` / `unreachable_device_unique_id` / `unreachable_device_channels` / `unreachable_device_sample_rate` / `unreachable_device_type`: `unreachable!` を本体とする。
- これらは C バックエンドの代役を務めるスタブであり、`AGENTS.md` のモック・スタブ禁止規約に反する。
- `OPS_EMPTY` を使うテストは `empty_device_list_is_not_an_error` だけであり、このテストが `FfiDeviceListImpl::enumerate` の唯一の単体テストである。
- `FfiDeviceListImpl::enumerate` の分岐のうち、空リストの扱い以外は未検証である。具体的には、負の `ret` の写像、負の `count`、`devices_ptr` が NULL で `count > 0`、未知の device_type、NULL デバイスポインタ、フィルタ不一致の各分岐にテストがない。
- `AudioDeviceType::from_ffi` / `AudioFormat::from_ffi` のテストは `src/device_ffi.rs` にあるが、これらはスタブを使わない。定義元の `src/common.rs` への移動は 0064 の範囲とする。

## 設計方針

- 空リストのテストを、FFI 関数を差し替えない純粋関数のテストに置き換える。`OPS_EMPTY` と上記 7 個の偽 FFI 関数は削除する。
- 純粋関数は `src/device_ffi.rs` の private 関数とし、テストは同ファイルの `#[cfg(test)] mod tests` に置く。
- `ret` の写像は `DeviceOps` のバックエンド別写像関数（0068 で追加）に委ねる。本 issue では、写像結果と `count` / `devices_ptr` の組み合わせを検証する純粋関数 `fn validate_enumerate_result(ret_error: Option<Error>, count: i32, devices_ptr_is_null: bool) -> Result<()>` を切り出す。この関数は、`ret_error` が `Some` ならそのエラーを返し、`count < 0` または `count > 0` かつ `devices_ptr_is_null` なら `Error::DeviceAccessDenied` を返し、それ以外は `Ok(())` を返す。`ret < 0` を `Error::DeviceAccessDenied` に写像する現行処理は、0068 の写像関数に置き換わるまで `enumerate` 側に残す。`ret < 0` を `Error` に写像する処理自体のテストは 0068 の完了条件に含まれる。
- 未知の device_type とフィルタの扱いを、`AudioDeviceType::from_ffi` を呼ぶ純粋関数 `fn select_device_type(device_type_raw: i32, filter: Option<AudioDeviceType>) -> Option<AudioDeviceType>` に切り出す。未知の device_type とフィルタ不一致は `None` にする。この関数のテストは、`AudioDeviceType::from_ffi` 自体のテスト（0064 で `src/common.rs` へ移る）を重複させず、`enumerate` の選別ロジックだけを対象にする。
- NULL デバイスポインタは `NonNull::new` で除外する。この分岐をテストするために、`fn non_null_device(ptr: *mut ffi::AudioDevice) -> Option<NonNull<ffi::AudioDevice>>` という薄い純粋関数に切り出し、`std::ptr::null_mut()` と `NonNull::dangling()` でテストする。
- 未知の device_type と NULL デバイスポインタをエラーまたは記録として扱うかどうかは 0082 の範囲とし、本 issue では現行の除外動作をテストする。0082 が先に実装された場合は、0082 の扱いに合わせて `select_device_type` と `non_null_device` の戻り値を調整する。
- 本 issue の時点でテスト用の `DeviceOps` を残す必要はない。将来 0065 の `Err(Error::NullPointer(...))` テストのように、FFI 関数ポインタを差し替えないと到達できない経路をテストする場合は、`CODEBASE.md` に規約の例外として理由を明文化する。
- 純粋関数とテストには、`CODEBASE.md` のコメント規約に従い日本語のコメントを付ける。
- `CHANGES.md` の `## develop` の `### misc` に、テスト用の偽 FFI 関数を削除し、列挙結果の検証を純粋関数のテストに置き換えた旨を `[UPDATE]` で追記する。

## 完了条件

- `src/device_ffi.rs` のテストから `OPS_EMPTY` と 7 個の偽 FFI 関数が無くなる。
- 空リストの扱い、`ret` の写像がエラーを返した場合、負の `count`、`count > 0` かつ `devices_ptr` が NULL の場合の組み合わせが、`validate_enumerate_result` のテストで検証されている。`ret < 0` を `Error` に写像する処理自体のテストは 0068 の完了条件に含まれる。
- 未知の device_type とフィルタ不一致の扱いが `select_device_type` のテストで検証されている。0082 が先に実装された場合は、0082 の扱いに合わせる。
- NULL デバイスポインタが `non_null_device` で除外されることがテストされている。0082 が先に実装された場合は、0082 の扱いに合わせる。
- 純粋関数とテストに日本語のコメントがある。
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリが追加されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が通る。
- CI の全構成 (CoreAudio / PulseAudio / PipeWire / WASAPI) で `cargo test --workspace` が通る。
