# write_playback_frame_to_buffer の防御的修正

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-write-playback-frame-defensive
- Polished: {YYYY-MM-DD}

## 目的

`src/common.rs` の `write_playback_frame_to_buffer` に存在する 2 つの問題を修正する。

1. `dst_channels == 0` のときゼロ除算で panic する
2. 同一フォーマットパスと変換パスで部分フレームの扱いが不整合

## 優先度根拠

現在の呼び出し元は `channels > 0` を検証済みだが、`pub(crate)` 関数として防御的でない。部分フレームの不整合はデータ破損の原因になり得る。

## 現状

### ゼロ除算（src/common.rs:233-270）

```rust
let sample_size = dst_channels * bytes_per_sample; // dst_channels == 0 → 0
let copy_frames = copy_len / sample_size; // ゼロ除算 → panic
```

### 部分フレーム不整合

同一フォーマットパスは完全フレーム単位に切り詰めてコピーするが、F32↔S16 変換パスはサンプル単位で変換し、部分フレームのデータもバッファに書き込む。

## 完了条件

- `dst_channels == 0` の場合に 0 を返す
- 変換パスでもフレーム境界に切り詰めてから変換する
- `cargo clippy` / `cargo test` が通る

## 解決方法

1. 関数冒頭に `if dst_channels == 0 { return 0; }` を追加する
2. 変換パスで `copy_len` を `copy_len - (copy_len % dst_channels)` に切り詰めてから変換する
