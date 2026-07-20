# 規約違反の一括修正

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-convention-violations
- Polished: {YYYY-MM-DD}

## 目的

`/review-code` で検出された AGENTS.md / CLAUDE.md 規約違反を一括で修正する。

## 優先度根拠

規約違反はコードの品質・一貫性を損なう。一括修正で技術的負債を解消する。

## 現状

以下の規約違反が検出されている:

1. **`#[allow]` → `#[expect]`**: `src/ffi.rs`:1-4 の 4 件の lint 抑制が `#![allow(...)]`
2. **Copy Enum のメソッド**: `src/common.rs`:43 の `AudioFormat::bytes_per_sample` が `&self` で受けている（規約は `self`）
3. **`.unwrap()` → `.expect()`**: `src/device_ffi.rs` テスト内 4 箇所、`pbt/tests/prop_capture.rs` 2 箇所、`pbt/tests/prop_playback.rs` 2 箇所
4. **テストの expect メッセージ英語 → 日本語**: `pbt/tests/prop_playback.rs`:11,23、`pbt/tests/prop_error.rs`:7
5. **AudioFormat doc コメント英語 → 日本語**: `src/common.rs`:22-25
6. **audio.h AUDIO_FORMAT_* コメント英語 → 日本語**: `src/audio.h`:14-15
7. **依存バージョン指定メジャーのみ → マイナーまで**: `pbt/Cargo.toml`:7 (`proptest = "1"`)、`fuzz/Cargo.toml`:22 (`arbitrary = { version = "1", ... }`)
8. **依存用途コメントなし**: `pbt/Cargo.toml`:7、`fuzz/Cargo.toml`:22-23

## 完了条件

- 上記 8 項目の規約違反がすべて修正される
- `cargo fmt --all -- --check` / `cargo clippy` / `cargo test` が通る

## 解決方法

各項目を順に修正する。いずれも機械的な修正であり、ロジック変更は伴わない。
