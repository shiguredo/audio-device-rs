# PBT から「パニックしないこと」だけを検証するテストを削除する

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## 概要

`pbt/tests/prop_capture.rs` に「任意入力でパニックしないことだけを検証するテスト」が存在し、CLAUDE.md のテスト規約に違反している。

## 対象箇所

- `pbt/tests/prop_capture.rs:72-78` — `as_s16_never_panics` テスト
- 同ファイルの `as_f32_never_panics` テスト（同一セクション内）

## 根拠

CLAUDE.md は以下を明記している:

> PBT に「任意入力でパニックしないことだけを検証するテスト」を書かない（fuzzing の役割）

パニック安全性の検証は Fuzzing の責務であり、PBT で行うべきではない。これらのテストは戻り値を無視しており、プロパティの検証になっていない。

## 対応方針

上記 2 つのテスト関数（`as_s16_never_panics` および `as_f32_never_panics`）を `pbt/tests/prop_capture.rs` から削除する。

その他のテスト（`valid_s16_frame_returns_some`, `valid_f32_frame_returns_some`, `short_data_returns_none`, `non_positive_metadata_returns_none`）はプロパティを正しく検証しているため維持する。

## 他 issue との依存関係

- パニック安全性の検証は **0015** (add-fuzzing-targets) で追加する fuzzing ターゲットでカバーする。本 issue は 0015 と同時に適用するのが望ましい

## CHANGES.md への追記

`## develop` セクションの `### misc` サブセクションに以下のエントリを追記する:

```
- [FIX] PBT から「パニックしないこと」のみを検証するテストを削除する
  - @ユーザー名
```
