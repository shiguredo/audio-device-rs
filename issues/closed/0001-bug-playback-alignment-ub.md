# Windows 再生経路の Vec<u8> → f32/i16 再解釈でアライメント違反による UB

Created: 2026-04-03
Model: Opus 4.6

## 概要

`playback_windows.rs` のフォーマット変換処理で `Vec<u8>` のポインタを `*const f32` / `*const i16` にキャストし `std::slice::from_raw_parts` で再解釈している。`Vec<u8>` のアライメントは 1 バイトであり、`f32` (4 バイト) / `i16` (2 バイト) のアライメント要件を満たさない可能性がある。これは未定義動作である。

## 根拠

`PlaybackFrame::from_s16` / `from_f32` は `to_le_bytes()` で各サンプルをバイト列に変換して `Vec<u8>` を構築する。このバッファのアライメントは 1 であり、`from_raw_parts` が要求するアライメント保証がない。x86 ではアライメント違反でクラッシュしにくいが、Rust の言語仕様上は UB であり、コンパイラの最適化によって壊れる可能性がある。

## 該当箇所

- `src/playback_windows.rs:415` (F32 → S16 変換の `from_raw_parts`)
- `src/playback_windows.rs:429` (S16 → F32 変換の `from_raw_parts`)

## 修正方針

`from_raw_parts` の代わりに `read_unaligned` でサンプルを 1 つずつ読み取る。

## 解決方法

Completed: 2026-04-03

`playback_windows.rs` の F32→S16 変換と S16→F32 変換で `std::slice::from_raw_parts` を廃止し、`ptr::read_unaligned` でサンプルを 1 つずつ読み取るように変更した。これにより `Vec<u8>` のアライメントに依存しなくなり UB を除去した。
