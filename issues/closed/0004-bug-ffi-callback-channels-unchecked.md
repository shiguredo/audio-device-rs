# FFI コールバックの channels 未検証によるバッファサイズ計算の不正

Created: 2026-04-03
Model: Opus 4.6

## 概要

`capture.rs` の `frame_callback` で FFI 由来の `channels` を検証せずにバッファサイズ計算に使用しており、`channels <= 0` の場合に `i32` の積が不正値となり `from_raw_parts` で範囲外読み取りが発生する。

## 再現条件

1. FFI レイヤーから `channels = 0` または負値が渡される
2. `data_size = (frames * channels * bytes_per_sample) as usize` が不正な値になる
3. `from_raw_parts` が範囲外メモリを参照する

## 影響

- debug build: panic
- release build: wrap した巨大な `usize` で `from_raw_parts` が OOB 読み取り (UB)

## 対象箇所

- `src/capture.rs:258` — `frame_callback` の早期リターン条件に `channels <= 0` がない
- `src/capture_windows.rs:467` — `frames_available as i32 * channels * bytes_per_sample` の符号付き積
- `src/playback_windows.rs:418` — 同上

## 解決方法

- `capture.rs`: 早期リターン条件に `channels <= 0` を追加し、バッファサイズ計算を `usize` ベースの `checked_mul` に変更した
- `capture_windows.rs` / `playback_windows.rs`: `as i32` キャストを廃止し、`usize` ベースの `checked_mul` に変更した。オーバーフロー時はバッファを解放して `continue` する

Completed: 2026-04-03
