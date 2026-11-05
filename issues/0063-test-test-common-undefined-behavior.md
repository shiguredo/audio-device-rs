# tests/test_common.rs の未定義動作を除去する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-test-common-undefined-behavior
- Polished: {YYYY-MM-DD}

## 目的

`tests/test_common.rs` が未定義動作を含むスライスを生成している状態を解消する。

## 現状

- `from_s16_data_too_large_returns_err` と `from_f32_data_too_large_returns_err` は、`std::ptr::NonNull::dangling()` を指すまま `i32::MAX as usize + 1` 要素のスライスを `std::slice::from_raw_parts` で構成している。
- `slice::from_raw_parts` は「`len * size_of::<T>()` バイトを読み取れる有効な領域」を要求するため、参照を作った時点で未定義動作である。32 bit ターゲットでは長さが `isize::MAX` も超える。
- `unsafe` に SAFETY コメントが無く、ファイル全体にモジュール doc とテストごとのコメントも無い (`AGENTS.md` のコメント重視の規約に反する)。

## 設計方針

- 長さ検査だけを純粋関数 (`fn checked_sample_count(len: usize, channels: i32) -> Result<i32>` など) に切り出し、その関数の単体テストに置き換える。
- どうしても `PlaybackFrame::from_s16` / `from_f32` 自体を対象にしたい場合は、実際に確保した領域を使うテストを `#[ignore]` 付きで分離するなど、Safety 契約を満たす形にする。
- ファイルと各テストに日本語のコメントを追加する。

## 完了条件

- 未定義動作を含むスライス生成が無くなる。
- `Error::DataTooLarge` の経路が未定義動作なしで検証されている。
- モジュール doc とテストコメントが追加されている。
- `cargo test --workspace` が通る (可能なら Miri でも確認する)。
