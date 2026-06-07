# PlaybackFrame::from_s16/from_f32 での整数オーバーフローを防ぐ

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

bug

## 概要

`PlaybackFrame::from_s16()` / `from_f32()` において、`data.len() as i32 / channels` の `as i32` 変換で整数オーバーフローが発生する可能性がある。

## 対象箇所

- `src/playback.rs:28` — `let frames = data.len() as i32 / channels;`
- `src/playback.rs:55` — `let frames = data.len() as i32 / channels;`
- `src/playback_windows.rs:36` — `let frames = data.len() as i32 / channels;`
- `src/playback_windows.rs:55` — `let frames = data.len() as i32 / channels;`

## 根拠

`data.len()` は `usize` (64 bit 環境では最大約 1.8e19)。`i32` の最大値は約 21 億。`as i32` の時点でオーバーフローしうる。例えば `&[i16]` は `isize::MAX / 2` 要素 (約 46 京) を持ちうるため、`as i32` でラップアラウンドして負数を含む不正な `frames` 値が生成される。後続の `flat_map` によるバイト列生成で不正なデータが生成される。

## 再現条件

- `data.len()` が `i32::MAX` を超えるサイズのスライスが渡された場合
- デバッグビルドでは `overflowing cast` でパニック、リリースビルドではラップアラウンド

## 対応方針

`i32::try_from(data.len())` を使用し、変換失敗時は `Err` を返す。エラー種別として `Error` に `DataTooLarge` バリアントを追加することを検討する。
