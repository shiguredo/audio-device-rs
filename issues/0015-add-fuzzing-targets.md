# Fuzzing ターゲットを追加する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

add

## 概要

プロジェクトに `cargo-fuzz` を用いた fuzzing ターゲットが一切存在しない。CLAUDE.md に「Fuzzing は cargo-fuzz を使うこと」と明記されており、規約に違反している。

## 根拠

CLAUDE.md のテスト戦略において、Fuzzing は「任意入力に対するクラッシュ耐性（パニック安全性）」を検証する役割を担う。以下の関数群は任意バイト列や任意整数値を受け取るため、fuzzing による網羅的なテストが有効である。

## 追加すべき Fuzzing ターゲット候補

1. `AudioFrameOwned::as_s16()` — 任意の `Vec<u8>` + 任意の `frames`, `channels`, `sample_rate`, `format` を受け取る
2. `AudioFrameOwned::as_f32()` — 同上
3. `PlaybackFrame::from_s16()` — 任意の `Vec<i16>` + 任意の `channels`, `sample_rate` を受け取る
4. `PlaybackFrame::from_f32()` — 任意の `Vec<f32>` + 任意の `channels`, `sample_rate` を受け取る
5. `AudioFrame::as_s16()` / `as_f32()` — 借用版のラッパー経由

## 対応方針

1. `fuzz/` ディレクトリを作成し `fuzz/Cargo.toml` を追加する
2. 上記各ターゲットに対応する fuzz target (`fuzz/fuzz_targets/*.rs`) を作成する
3. `Makefile` の fuzzing 関連ターゲットが動作することを確認する
