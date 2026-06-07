# determine_audio_format の重複定義を統合する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

fix

## 概要

`capture_windows.rs` と `playback_windows.rs` で、完全に同一のロジックを持つ `determine_audio_format()` / `determine_playback_format()` が別々に定義されている。

## 対象箇所

- `src/capture_windows.rs:380-396` — `unsafe fn determine_audio_format()`
- `src/playback_windows.rs:345-361` — `unsafe fn determine_playback_format()`

両関数のロジックは完全に同一:
```rust
let format_tag = unsafe { (*wave_format).wFormatTag };
if format_tag == WAVE_FORMAT_IEEE_FLOAT as u16 {
    return AudioFormat::F32;
}
if format_tag == WAVE_FORMAT_EXTENSIBLE as u16 {
    let ext = wave_format as *const WAVEFORMATEXTENSIBLE;
    let sub_format = unsafe { std::ptr::addr_of!((*ext).SubFormat).read_unaligned() };
    if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
        return AudioFormat::F32;
    }
}
AudioFormat::S16
```

## 根拠

DRY 原則違反。関数名が異なるだけで実装が完全に重複している。修正時に片方だけ更新されるリスクがある。

## 対応方針

共通モジュール（例: `src/device_windows.rs` に `pub(crate)` として）に統合し、1 つの関数として定義する。両呼び出し元からは統合後の関数を参照する。
