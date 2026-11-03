# tests/test_common.rs の未定義動作を除去する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-test-common-undefined-behavior
- Polished: 2026-11-04

## 目的

`tests/test_common.rs` が未定義動作を含むスライスを生成している状態を解消する。

## 現状

- `from_s16_data_too_large_returns_err` と `from_f32_data_too_large_returns_err` は、`std::ptr::NonNull::dangling()` を指すまま `i32::MAX as usize + 1` 要素のスライスを `std::slice::from_raw_parts` で構成している。
- `slice::from_raw_parts` は「`len * size_of::<T>()` バイトを読み取れる有効な領域」を要求するため、参照を作った時点で未定義動作である。64 bit 環境の Miri では、`from_s16` 側は 4294967296 バイト、`from_f32` 側は 8589934592 バイトの範囲が dangling pointer であると報告される。32 bit ターゲットでは長さが `isize::MAX` も超える。
- `unsafe` に SAFETY コメントが無く、ファイル全体にモジュール doc とテストごとのコメントも無い (`AGENTS.md` のコメント重視の規約に反する)。
- `try_from_int_error_display_contains_data_too_large` は `Error` の `Display` と `From<TryFromIntError>` を検証しており、`src/common.rs` の型を対象にしていない。

## 設計方針

- `src/common.rs` の `PlaybackFrame::from_s16` / `from_f32` の長さ検査を、スライスを受け取らない private の純粋関数 `fn checked_sample_count(len: usize) -> Result<i32>` に切り出す。この関数は `i32::try_from(len)` の結果を返し、`len` が `i32` に収まらない場合は `Error::DataTooLarge` を返す。`channels` の検証と `channels` の倍数かどうかの検証は `from_s16` / `from_f32` に残す。
- `checked_sample_count` の単体テストは `src/common.rs` の `#[cfg(test)]` モジュールに置く。`checked_sample_count` はバックエンド feature に依存しないため、テストも `enable_*` に依存させず、`cargo test --no-default-features` でも実行できるようにする。`tests/` は公開 API に対してだけテストする (`shiguredo-rust` の「`tests/`・`pbt/`・`fuzz/` のテストは公開 API に対してだけ書くこと」と「private を対象とする単体テストは `src/<module>.rs` 内の `#[cfg(test)]` モジュールに書くこと」に従う)。
- `tests/test_common.rs` から 2 つの未定義動作テストを削除する。残る `try_from_int_error_display_contains_data_too_large` は `Error` の公開 API に対するテストなので `tests/test_error.rs` へ移す。`tests/test_common.rs` にテストが残らない場合はファイルを削除する (`shiguredo-rust` の「単体テストのファイル名は `tests/test_<module>.rs` とし、`src/<module>.rs` に対応させること」に従う)。
- `i32::MAX as usize + 1` 要素の実データを確保するテストは追加しない。`PlaybackFrame::from_s16` / `from_f32` の `Error::DataTooLarge` の経路は、巨大な領域を確保せず、純粋関数 `checked_sample_count` の単体テストで検証する (`shiguredo-rust` の「`#[ignore]` を使わないこと」に従う)。
- `src/common.rs` の新しい単体テストと `tests/test_error.rs` の移動したテストに日本語のコメントを追加する。コメントは `CODEBASE.md` の規約に従い、今のコードの性質だけを書く。`tests/test_error.rs` のモジュール doc は既存のものを維持する。

## 完了条件

- 未定義動作を含むスライス生成が無くなる。
- `Error::DataTooLarge` の経路が未定義動作なしで検証されている。
- `checked_sample_count` の単体テストが `src/common.rs` の `#[cfg(test)]` モジュールにあり、`cargo test --no-default-features` と `cargo test --workspace` で実行される。
- `tests/test_common.rs` から未定義動作を含むテストが無くなり、`try_from_int_error_display_contains_data_too_large` は `tests/test_error.rs` に移っている。`tests/test_common.rs` にテストが残らない場合は削除されている。
- `src/common.rs` の新しい単体テストと `tests/test_error.rs` の移動したテストに日本語のコメントがある。
- `cargo test --workspace` が通る。
- 可能なら `cargo +nightly miri test --no-default-features --lib` で未定義動作がないことを確認する。FFI バックエンドを含む workspace 全体は Miri で実行できないため、バックエンドを無効にした lib の単体テストだけを対象にする。
