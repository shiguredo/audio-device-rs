# PlaybackFrame の frames フィールド不一致

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-playback-frame-frames-mismatch
- Polished: {YYYY-MM-DD}

## 目的

`src/common.rs` の `PlaybackFrame::from_s16` / `from_f32` で、`data.len()` が `channels` で割り切れない場合に `frames` フィールドが実際のデータ量を正しく表さない問題を修正する。

## 優先度根拠

`frames` フィールドは公開 API の一部であり、データ量を正確に記述すべき。不一致は下流の処理でデータ破損や誤ったフレーム数計算を引き起こす。

## 現状

src/common.rs:186-196:

```rust
pub fn from_s16(data: &[i16], channels: i32, sample_rate: i32) -> Result<Self> {
    if channels <= 0 {
        return Err(Error::InvalidChannels);
    }
    let frames = i32::try_from(data.len())? / channels; // 切り捨て
    let bytes: Vec<u8> = data
        .iter()
        .flat_map(|&sample| sample.to_le_bytes())
        .collect(); // 全サンプルをバイト化（余りサンプルを含む）
```

例: `from_s16(&[1, 2, 3], 2, 48000)` → `frames = 1` だが `data` は 3 サンプル (6 バイト) を保持。

## 完了条件

- `data.len() % channels != 0` の場合にエラーを返すか、データを切り詰める
- PBT で新しい挙動が検証される
- `cargo clippy` / `cargo test` が通る

## 解決方法

`data.len() % channels as usize != 0` の場合に `Err` を返す（厳密な検証）。または `frames * channels` 個のサンプルにデータを切り詰める（寛容な処理）。いずれかを選択し、PBT で検証する。
