# PBT から「パニックしないこと」だけを検証するテストを削除する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

fix

## 概要

`pbt/tests/prop_capture.rs` に「任意入力でパニックしないことだけを検証するテスト」が存在し、CLAUDE.md のテスト規約に違反している。

## 対象箇所

- `pbt/tests/prop_capture.rs:72-78`

```rust
/// 任意の AudioFrameOwned に対して as_s16() / as_f32() が panic しない
#[test]
fn as_s16_never_panics(frame in arb_audio_frame_owned()) {
    let _ = frame.as_s16();
}

#[test]
fn as_f32_never_panics(frame in arb_audio_frame_owned()) {
    let _ = frame.as_f32();
}
```

## 根拠

CLAUDE.md は以下を明記している:

> PBT に「任意入力でパニックしないことだけを検証するテスト」を書かない（fuzzing の役割）

パニック安全性の検証は Fuzzing の責務であり、PBT で行うべきではない。戻り値を無視しており、プロパティの検証になっていない。

## 対応方針

上記 2 つのテストを削除する。パニック安全性は `0015-add-fuzzing-targets` で追加する fuzzing ターゲットでカバーする。
