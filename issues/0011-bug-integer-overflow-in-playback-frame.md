# PlaybackFrame::from_s16/from_f32 での整数オーバーフローを防ぐ

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## 概要

`PlaybackFrame::from_s16()` / `from_f32()` において、`data.len() as i32 / channels` の `as i32` 変換で整数オーバーフローが発生する可能性がある。`i32::try_from()` に置き換えてオーバーフロー時は `Err` を返す。

## 対象箇所

- `src/playback.rs:28` — `from_s16` 内の `data.len() as i32 / channels`
- `src/playback.rs:47` — `from_f32` 内の `data.len() as i32 / channels`
- `src/playback_windows.rs:36` — `from_s16` 内の `data.len() as i32 / channels`
- `src/playback_windows.rs:55` — `from_f32` 内の `data.len() as i32 / channels`

注意: `src/playback.rs` と `src/playback_windows.rs` の `PlaybackFrame` は現在完全に重複しており、両方に同じ修正を適用する必要がある。

## 根拠

`data.len()` は `usize`（64 ビット環境では最大約 1.8e19）。`i32` の最大値は約 21 億。`as i32` の時点でオーバーフローしうる。debug ビルドでは overflow check によりパニック、release ビルド（デフォルトの overflow-checks=off）ではラップアラウンドし、負数を含む不正な `frames` 値が生成される。後続の `flat_map` によるバイト列生成で不正なデータが生成される。

## 再現手順

```rust
// from_s16 のオーバーフロー再現（debug ビルドで panic）
// 64 ビット環境で約 4.3 GB のメモリが必要なため、完全な再現は非現実的だが、
// overflow-checks=on な debug ビルドではパニックする
let huge_data: Vec<i16> = (0..(i32::MAX as usize + 1)).map(|_| 0).collect();
// ↑ この Vec 自体の構築で OOM になるため現実には発生しづらいが、
// FFI 経由で巨大なスライスが渡された場合に発現しうる
```

## 対応方針

### 1. Error バリアントの追加

`src/error.rs` に以下を追加する:

```rust
pub enum Error {
    // ... 既存バリアント ...
    DataTooLarge,
}
```

`Display` 実装:

```rust
Error::DataTooLarge => write!(f, "data too large: exceeds i32::MAX"),
```

### 2. from_s16 / from_f32 の修正

**両ファイル（`src/playback.rs` と `src/playback_windows.rs`）に同じ修正を適用する**:

```rust
// 変更後
pub fn from_s16(data: &[i16], channels: i32, sample_rate: i32) -> Result<Self> {
    if channels <= 0 {
        return Err(Error::InvalidChannels);
    }
    let frames = i32::try_from(data.len())
        .map_err(|_| Error::DataTooLarge)?
        / channels;
    let bytes: Vec<u8> = data
        .iter()
        .flat_map(|&sample| sample.to_le_bytes())
        .collect();
    Ok(Self {
        data: bytes,
        frames,
        channels,
        sample_rate,
        format: AudioFormat::S16,
    })
}

pub fn from_f32(data: &[f32], channels: i32, sample_rate: i32) -> Result<Self> {
    if channels <= 0 {
        return Err(Error::InvalidChannels);
    }
    let frames = i32::try_from(data.len())
        .map_err(|_| Error::DataTooLarge)?
        / channels;
    let bytes: Vec<u8> = data
        .iter()
        .flat_map(|&sample| sample.to_le_bytes())
        .collect();
    Ok(Self {
        data: bytes,
        frames,
        channels,
        sample_rate,
        format: AudioFormat::F32,
    })
}
```

### 3. CHANGES.md への追記

`## develop` セクションに以下のエントリを追記する:

```
- [FIX] PlaybackFrame::from_s16/from_f32 での usize→i32 キャストによる整数オーバーフローを修正する
  - @ユーザー名
```

### 4. テスト戦略

- PBT による検証が適切。`pbt/tests/prop_playback.rs` にテストケースを追加する
- 追加テストケース:
  - `data.len()` が `i32::MAX` を超える場合に `Err` が返ること（テスト用の Strategy で巨大なスライスは生成できないため、代わりに `from_s16` の `i32::try_from` 呼び出しが含まれていることを確認する単体テストも併用する）
  - `data.len()` が `i32::MAX` 以下の場合に正常に動作すること（既存 PBT でカバー）
- 単体テスト: 境界値として `data.len() == i32::MAX as usize` のケース（Ok）、`data.len() == i32::MAX as usize + 1` のケース（Err）を `tests/test_playback.rs` に追加する

## 他 issue との依存関係

- **0009** (update-extract-common-audio-types) は `PlaybackFrame` を `src/common.rs` に移動する。0009 が先に適用された場合、本 issue の修正対象は `src/common.rs` 内の `PlaybackFrame::from_s16/from_f32` になる。いずれの順序でも修正内容自体は同一であり、競合は発生しない
