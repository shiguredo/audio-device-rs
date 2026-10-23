# PlaybackFrame の frames フィールド不一致

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-10-23
- Model: Qwen 3
- Branch: feature/fix-playback-frame-frames-mismatch
- Polished: 2026-07-21

## 目的

`src/common.rs` の `PlaybackFrame::from_s16` / `from_f32` で、`data.len()` が `channels` で割り切れない場合に `frames` フィールドが実際のデータ量を正しく表さない問題を修正する。

## 優先度根拠

`frames` フィールドは公開 API の一部であり、データ量を正確に記述すべき。不一致は下流の処理でデータ破損や誤ったフレーム数計算を引き起こす。

## 現状

### from_s16（src/common.rs:211-227）

```rust
pub fn from_s16(data: &[i16], channels: i32, sample_rate: i32) -> Result<Self> {
    if channels <= 0 {
        return Err(Error::InvalidChannels);
    }
    let frames = i32::try_from(data.len())? / channels;
    let bytes: Vec<u8> = data
        .iter()
        .flat_map(|&sample| sample.to_le_bytes())
        .collect();
```

`frames` は切り捨て除算で計算されるが、`bytes` は余りサンプルを含む全サンプルをバイト化する。

### from_f32（src/common.rs:230-246）

```rust
pub fn from_f32(data: &[f32], channels: i32, sample_rate: i32) -> Result<Self> {
    if channels <= 0 {
        return Err(Error::InvalidChannels);
    }
    let frames = i32::try_from(data.len())? / channels;
    let bytes: Vec<u8> = data
        .iter()
        .flat_map(|&sample| sample.to_le_bytes())
        .collect();
```

`from_s16` と同一の構造で同一の問題を持つ。

例: `from_s16(&[1, 2, 3], 2, 48000)` → `frames = 1` だが `data` は 3 サンプル (6 バイト) を保持。`frames * channels * bytes_per_sample != data.len()` となる。

## 設計方針

`data.len() % channels != 0` の場合に `Err` を返す（厳密な検証）。公開 API のコンストラクタであり、既に `Result` を返しているため、エラーによる拒否が自然。切り詰め案は呼び出し側にデータ損失を通知できないため不採用。

## 完了条件

- `data.len() % channels != 0` の場合にエラーを返す
- PBT で「割り切れない入力でエラーを返す」「割り切れる入力で `frames * channels == frame.data.len() / bytes_per_sample` が成り立つ」を検証する
- 既存の PBT テスト（`pbt/tests/prop_playback.rs`）が整合入力（`data.len() % channels == 0`）を生成するよう修正される
- `cargo clippy` / `cargo test` が通る

## 解決方法

`src/common.rs` の `PlaybackFrame::from_s16` / `from_f32` を次のとおり修正した。

- `src/error.rs` に `Error::InvalidDataLength` を追加した。`Display` は `invalid data length: must be a multiple of channels` を返す
- `PlaybackFrame::from_s16` / `from_f32` で、引数 `data` の要素数が `channels` の倍数でない場合に `Error::InvalidDataLength` を返すようにした。`frames` が実データのサンプル数と食い違う `PlaybackFrame` は生成されない
- 引数 `data` の要素数が `i32` に収まらない場合は、従来どおり `Error::DataTooLarge` を返す
- `PlaybackFrame::from_s16` / `from_f32` の doc コメントに、引数の解釈とエラー条件を記載した
- `pbt/tests/prop_common.rs` の既存 PBT を `channels` の倍数のサンプル数を生成するよう修正し、`frames * channels` が実データのサンプル数と一致することを検証する PBT と、非整列入力で `Error::InvalidDataLength` を返すことを検証する PBT を追加した
- `pbt/tests/prop_error.rs` と `tests/test_error.rs` に `Error::InvalidDataLength` のテストを追加した
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追記した

`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` がすべて成功することを確認した。

## 後方互換

後方互換性の維持は不要。公開 API の動作変更として、非整列データ（`data.len() % channels != 0`）を渡した場合はエラーを返す
