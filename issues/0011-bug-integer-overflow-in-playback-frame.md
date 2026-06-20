# PlaybackFrame::from_s16/from_f32 での整数オーバーフローを防ぐ

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-20

## カテゴリ

bug

## 概要

`PlaybackFrame::from_s16()` / `from_f32()` において、`data.len() as i32 / channels` の `as i32` 変換で整数オーバーフローが発生する可能性がある。`i32::try_from()` に置き換えてオーバーフロー時は `Err` を返す。

## 現状

`src/common.rs` の `PlaybackFrame::from_s16` と `PlaybackFrame::from_f32` で `data.len() as i32 / channels` が使用されている。

`src/error.rs` には `DataTooLarge` バリアントが unit バリアントとして既に定義されている（行 11）が、`Display` 実装の match アームが欠落している。Rust の exhaustive match 規則により、現在の `src/error.rs` はコンパイルエラーの状態である（`E0004: non-exhaustive patterns`）。また `From<std::num::TryFromIntError>` 実装も存在しない。このコンパイルエラーと `From` 実装の欠落を本 issue で併せて解消する。

## 対象箇所

- `src/common.rs` — `PlaybackFrame::from_s16` 内の `data.len() as i32 / channels`
- `src/common.rs` — `PlaybackFrame::from_f32` 内の `data.len() as i32 / channels`
- `src/error.rs` — `DataTooLarge` バリアントの変更、`Display` 実装の追加、`From<std::num::TryFromIntError>` 実装の追加
- `pbt/tests/prop_playback.rs` — 既存テスト内の `expected_frames = data.len() as i32 / channels` にも同一パターンが存在する（strategy の範囲が `0..=512` に制限されているため現状では問題が表面化しないが、修正が必要）
- `CHANGES.md` — `## develop` セクションへのエントリ追記

## 根拠

`data.len()` は `usize`（64 ビット環境では最大約 1.8e19）。`i32` の最大値は約 21 億。`as i32` の時点でオーバーフローしうる。debug ビルドでは overflow check によりパニック、release ビルド（デフォルトの overflow-checks=off）ではラップアラウンドし、負数を含む不正な `frames` 値が生成される。この不正な `frames` 値により、`as_s16()` / `as_f32()` のバッファ長検証が破綻し、不正なスライスが構築される。

## 再現手順

`i32::MAX as usize + 1` 要素の `Vec<i16>` の構築には約 4.3 GB のメモリが必要であり、現実的な環境では OOM となる。コードレビューにより `as i32` キャストを確認することで脆弱性を検出できる。また fuzzing（cargo-fuzz）により任意の入力から `from_s16`/`from_f32` を呼び出すことで、オーバーフロー経路を検証可能である。

## 優先度根拠

堅牢性の優先。debug ビルドでのパニック、release ビルドでの不正データ生成はいずれも深刻なバグである。既存の PBT テストはデータサイズが 512 以下に制限されているため発見されていない。

## 設計方針

- `as i32` を `i32::try_from(data.len())?` に置き換える
- オーバーフロー時は `Error::DataTooLarge` を返す
- `DataTooLarge` は既存の unit バリアントを `DataTooLarge(std::num::TryFromIntError)` に変更する。現状このバリアントを構築するコードパスは存在しないため、後方互換性への影響はない
- `From<std::num::TryFromIntError>` を実装し、`?` 演算子によるエラー伝播を可能にする

## 完了条件

- `from_s16` / `from_f32` の `as i32` が `i32::try_from()` に置き換わっている
- オーバーフロー時に `Error::DataTooLarge` が返る
- `src/error.rs` のコンパイルエラー（`DataTooLarge` の `Display` アーム欠落）が解消している
- `From<std::num::TryFromIntError> for Error` が実装されている
- 既存 PBT / 単体テストがすべてパスする
- 既存 PBT のテストコードの `as i32` も `i32::try_from` ベースに修正されている
- 境界値の単体テストが `tests/test_common.rs` に追加されている
- Fuzzing ターゲットが追加されている（または issue 0015 完了後に追加する旨の TODO が残されている）
- `CHANGES.md` の `## develop` に `[FIX]` エントリが既存エントリの直後に追記されている

## 対応方針

### 1. Error バリアントの変更と関連実装の追加

`src/error.rs` の `DataTooLarge` バリアントを unit バリアントから `TryFromIntError` を内包するバリアントに変更する。併せて `Display` 実装の match アームと `From<std::num::TryFromIntError>` 実装を追加する。

`DataTooLarge` のバリアント変更:

```rust
pub enum Error {
    // ... 既存バリアント ...
    DataTooLarge(std::num::TryFromIntError), // unit バリアントから変更
}
```

`Display` 実装に以下を追加する:

```rust
Error::DataTooLarge(e) => write!(f, "data too large: {}", e),
```

`From` 実装を追加する:

```rust
impl From<std::num::TryFromIntError> for Error {
    fn from(e: std::num::TryFromIntError) -> Self {
        Error::DataTooLarge(e)
    }
}
```

注意: `TryFromIntError` は `Copy + Clone` を実装しているため、`#[derive(Debug, Clone)]` の導出は引き続き有効である。

### 2. from_s16 / from_f32 の修正

`src/common.rs` の `PlaybackFrame::from_s16` と `PlaybackFrame::from_f32` を修正する:

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
    let frames = i32::try_from(data.len())? / channels;
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

### 3. 既存 PBT テストの修正

`pbt/tests/prop_playback.rs` の `from_s16_valid_channels_returns_ok` および `from_f32_valid_channels_returns_ok` 内の期待値計算を修正する:

```rust
// 変更前
let expected_frames = data.len() as i32 / channels;
// 変更後
// strategy の data サイズは 0..=512 に制限されているため unwrap は安全
let expected_frames = i32::try_from(data.len()).expect("data len must fit in i32") / channels;
```

`prop_playback.rs` のリネーム（`prop_common.rs` への変更）は本 issue のスコープ外とする。

### 4. テスト戦略

#### 既存テストの修正

- 既存 PBT: `pbt/tests/prop_playback.rs` の期待値計算式を `i32::try_from` ベースに修正する

#### 新規追加テスト

##### 単体テスト (`tests/test_common.rs` を新規作成)

`from_s16` / `from_f32` に境界値（`data.len() == i32::MAX as usize + 1`）のデータを渡すテストを追加する。`Vec` 構築は OOM になるため、dangling ポインタからスライスを構築して渡す（`from_s16` は `data.len()` の直後に `?` で早期リターンするため、ポインタの deref 前に失敗し安全）:

```rust
#[test]
fn from_s16_data_too_large_returns_err() {
    let len = i32::MAX as usize + 1;
    let data: &[i16] = unsafe {
        std::slice::from_raw_parts(std::ptr::NonNull::dangling().as_ptr(), len)
    };
    let result = PlaybackFrame::from_s16(data, 1, 48000);
    assert!(matches!(result, Err(Error::DataTooLarge(_))));
}

#[test]
fn from_f32_data_too_large_returns_err() {
    let len = i32::MAX as usize + 1;
    let data: &[f32] = unsafe {
        std::slice::from_raw_parts(std::ptr::NonNull::dangling().as_ptr(), len)
    };
    let result = PlaybackFrame::from_f32(data, 1, 48000);
    assert!(matches!(result, Err(Error::DataTooLarge(_))));
}
```

正常系テストとして、`data.len()` が `i32::MAX` 以下の場合に `from_s16` / `from_f32` が `Ok` を返すことを確認する（引数例: `channels = 1, sample_rate = 48000`）。

また以下を確認する:

- `channels <= 0` の場合に `Err(Error::InvalidChannels)` を返すこと（既存 PBT でカバー済みだが、念のため単体テストにも追加）
- `Error::from(std::num::TryFromIntError).to_string()` に `"data too large"` が含まれること

##### PBT

`pbt/tests/prop_playback.rs` の既存 strategy（`data` のサイズ `0..=512`、`channels in 1..=32`）の範囲ではオーバーフローは発生しえない。PBT 側に新規テストケースの追加は不要。

##### Fuzzing

`cargo-fuzz` で `from_s16` / `from_f32` に任意のバイト列を入力として与え、パニックしないことを検証する fuzz ターゲットを `fuzz/fuzz_targets/playback_frame.rs` として追加する。

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列を i16 スライスとして再解釈して from_s16 を呼ぶ
    if data.len() >= 2 {
        let (_prefix, aligned, _suffix) = unsafe { data.align_to::<i16>() };
        let _ = unsafe {
            shiguredo_audio_device::PlaybackFrame::from_s16(aligned, 1, 48000)
        };
    }
    // 任意のバイト列を f32 スライスとして再解釈して from_f32 を呼ぶ
    if data.len() >= 4 {
        let (_prefix, aligned, _suffix) = unsafe { data.align_to::<f32>() };
        let _ = unsafe {
            shiguredo_audio_device::PlaybackFrame::from_f32(aligned, 1, 48000)
        };
    }
});
```

注意: 本 fuzz ターゲットの追加には `fuzz/` ディレクトリのセットアップが必要であり、issue 0015（`add-fuzzing-targets`）で整備される `cargo-fuzz` 基盤に依存する。0015 の完了を待つか、本 issue 内で先行して fuzz ターゲットを作成するかは実装時の判断とする。

### 5. CHANGES.md への追記

`## develop` セクションの既存 `[FIX]` エントリの直後に以下のエントリを追記する:

```
- [FIX] PlaybackFrame::from_s16/from_f32 での usize から i32 へのキャストによる整数オーバーフローを防止する
  - @melpon
```
