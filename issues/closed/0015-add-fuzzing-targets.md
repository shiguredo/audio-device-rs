# Fuzzing ターゲットを追加する

Created: 2026-06-07
Completed: 2026-06-21
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

add

## 概要

プロジェクトに `cargo-fuzz` を用いた fuzzing ターゲットが一切存在しない。CLAUDE.md に「Fuzzing は cargo-fuzz を使うこと」と明記されており、規約に違反している。

## 根拠

CLAUDE.md のテスト戦略において、Fuzzing は「任意入力に対するクラッシュ耐性（パニック安全性）」を検証する役割を担う。以下の関数群は任意バイト列や任意整数値を受け取るため、fuzzing による網羅的なテストが有効である。

## 追加すべき Fuzzing ターゲット

1. `AudioFrameOwned::as_s16()` — 任意の `[u8]` + 任意の `frames`, `channels`, `sample_rate`, `format` を受け取る
2. `AudioFrameOwned::as_f32()` — 同上
3. `PlaybackFrame::from_s16()` — 任意の `[i16]` + 任意の `channels`, `sample_rate` を受け取る
4. `PlaybackFrame::from_f32()` — 任意の `[f32]` + 任意の `channels`, `sample_rate` を受け取る

## 対応方針

### 1. `fuzz/` ディレクトリを新設する

```toml
# fuzz/Cargo.toml
[package]
name = "shiguredo_audio_device-fuzz"
version = "0.0.0"
edition = "2024"
publish = false

[package.metadata]
cargo-fuzz = true

[dependencies]
libfuzzer-sys = "0.4"
arbitrary = { version = "1", features = ["derive"] }

[dependencies.shiguredo_audio_device]
path = ".."

[dev-dependencies]
```

### 2. Fuzzing ターゲットを作成する

`fuzz/fuzz_targets/audio_frame_as_s16.rs`:

```rust
#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

#[derive(Debug, Arbitrary)]
struct Input {
    data: Vec<u8>,
    frames: i32,
    channels: i32,
    sample_rate: i32,
    format: u8, // 0 = S16, 1 = F32
    timestamp_us: i64,
}

fuzz_target!(|input: Input| {
    let format = if input.format % 2 == 0 {
        shiguredo_audio_device::AudioFormat::S16
    } else {
        shiguredo_audio_device::AudioFormat::F32
    };
    let frame = shiguredo_audio_device::AudioFrameOwned {
        data: input.data,
        frames: input.frames,
        channels: input.channels,
        sample_rate: input.sample_rate,
        format,
        timestamp_us: input.timestamp_us,
    };
    let _ = frame.as_s16();
    let _ = frame.as_f32();
});
```

`fuzz/fuzz_targets/playback_frame_from_s16.rs`:

```rust
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: (Vec<i16>, i32, i32)| {
    let (data, channels, sample_rate) = data;
    let _ = shiguredo_audio_device::PlaybackFrame::from_s16(&data, channels, sample_rate);
});
```

`fuzz/fuzz_targets/playback_frame_from_f32.rs`:

```rust
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: (Vec<f32>, i32, i32)| {
    let (data, channels, sample_rate) = data;
    let _ = shiguredo_audio_device::PlaybackFrame::from_f32(&data, channels, sample_rate);
});
```

### 3. Makefile で fuzzing が動作することを確認する

既存の Makefile には fuzz 関連ターゲットが存在する。`make fuzz` が全ターゲットをビルド・実行できることを確認する。

## 他 issue との依存関係

- **0016** (fix-remove-panic-never-pbt) は `pbt/tests/prop_capture.rs` からパニック安全性テストを削除する。本 issue の fuzzing ターゲットが削除分のカバレッジを置き換えるため、0016 と同時または直後に適用する

## CHANGES.md への追記

`## develop` セクションに以下のエントリを追記する:

```
- [ADD] cargo-fuzz を用いた fuzzing ターゲットを追加する
  - @ユーザー名
```

## 解決方法

- `fuzz/Cargo.toml` に `arbitrary` 依存を追加し、`[[bin]]` エントリを新規 3 ターゲット用に更新した
- 旧 `fuzz/fuzz_targets/playback_frame.rs` を削除し、以下 3 つの fuzz ターゲットを新規作成した:
  - `fuzz/fuzz_targets/audio_frame_as_s16.rs` — `AudioFrameOwned::as_s16()` および `as_f32()` を fuzz
  - `fuzz/fuzz_targets/playback_frame_from_s16.rs` — `PlaybackFrame::from_s16()` を fuzz
  - `fuzz/fuzz_targets/playback_frame_from_f32.rs` — `PlaybackFrame::from_f32()` を fuzz
- Makefile に `fuzz: fuzzing` エイリアスを追加した
- `cargo +nightly fuzz check` および `cargo clippy --workspace -- -D warnings` が通過することを確認した
