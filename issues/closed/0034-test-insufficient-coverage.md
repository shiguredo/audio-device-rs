# テスト不足の解消

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-10-23
- Model: Qwen 3
- Branch: feature/add-insufficient-test-coverage
- Polished: {YYYY-MM-DD}

## 目的

`/review-code` で検出されたテスト不足を解消する。

## 優先度根拠

主要ロジックに対するテストの欠落は、リグレッションの検出を遅らせる。

## 現状

以下のテスト不足が検出されている:

1. **PBT ファイル名不一致**: `pbt/tests/prop_capture.rs` は `AudioFrameOwned`（`src/common.rs` 定義）をテストしており `src/capture.rs` に対応しない。`pbt/tests/prop_playback.rs` も `PlaybackFrame`（`src/common.rs` 定義）をテストしており `src/playback.rs` に対応しない。→ `prop_common.rs` にリネーム・統合する
2. **Error::InvalidChannels PBT 欠落**: `pbt/tests/prop_error.rs` の `arb_error()` に `Error::InvalidChannels` が含まれていない
3. **write_playback_frame_to_buffer 直接テストなし**: `src/common.rs` に `#[cfg(test)] mod tests` が存在しない。`playback_ffi.rs` の `playback_callback` 経由の間接テストのみ
4. **AudioFrame::to_owned() テストなし**: 公開 API に対応するテストが単体テスト・PBT のいずれにも存在しない

## 完了条件

- PBT ファイルが `prop_common.rs` にリネーム・統合される
- `arb_error()` に `Error::InvalidChannels` が追加される
- `write_playback_frame_to_buffer` の直接テストが追加される
- `AudioFrame::to_owned()` のラウンドトリップテストが追加される
- `cargo test --workspace` が通る

## 解決方法

issue に列挙された 4 項目は、develop 上の規約準拠作業で既に解消済みであった。追加のコード変更は不要であった。

- `pbt/tests/prop_capture.rs` と `pbt/tests/prop_playback.rs` は `pbt/tests/prop_common.rs` に統合済みである
- `pbt/tests/prop_error.rs` の `ERROR_SAMPLERS` に `Error::InvalidChannels` が含まれている
- `src/common.rs` に `#[cfg(test)] mod tests` があり、`write_playback_frame_to_buffer` の空データ、部分フレーム、F32 パディング、マルチチャンネル変換を直接テストしている
- `pbt/tests/prop_common.rs` の `prop_audio_frame_owned_roundtrip` が `as_frame()` → `to_owned()` のラウンドトリップを検証している

`cargo test --workspace` が成功することを確認した。
