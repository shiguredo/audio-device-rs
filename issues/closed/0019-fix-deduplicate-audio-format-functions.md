# determine_audio_format の重複定義を統合する

Created: 2026-06-07
Completed: 2026-06-21
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## 概要

`capture_windows.rs` と `playback_windows.rs` で、完全に同一のロジックを持つ `determine_audio_format()` / `determine_playback_format()` が別々に定義されている。

## 対象箇所

- `src/capture_windows.rs:380-396` — `unsafe fn determine_audio_format()`
- `src/playback_windows.rs:345-361` — `unsafe fn determine_playback_format()`

## 根拠

DRY 原則違反。関数名が異なるだけで実装が完全に重複している。修正時に片方だけ更新されるリスクがある。

## 対応方針

### 1. `src/device_windows.rs` に統合する

両関数のロジックは完全に同一であるため、`src/device_windows.rs` に `pub(crate)` 関数として統合する:

```rust
// src/device_windows.rs に追加
/// オーディオフォーマットを判定（共通関数）
pub(crate) unsafe fn determine_audio_format(wave_format: *const WAVEFORMATEX) -> crate::device_windows::AudioFormat {
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
}
```

### 2. 呼び出し側の変更

- `capture_windows.rs`: `determine_audio_format()` を削除し、`crate::device_windows::determine_audio_format()` を呼び出す
- `playback_windows.rs`: `determine_playback_format()` を削除し、`crate::device_windows::determine_audio_format()` を呼び出す

### 3. 注意点

- `determine_audio_format()` の戻り型は `AudioFormat`。本 issue では `device_windows` モジュールで定義された型をそのまま使う
- **0009** (update-extract-common-audio-types) が先に適用された場合、`AudioFormat` 型は `common` から import されているため import パスが変わる可能性がある。0009 適用後には `use crate::common::AudioFormat;` を使用する

## CHANGES.md への追記

`## develop` セクションの `### misc` サブセクションに以下のエントリを追記する:

```
- [FIX] determine_audio_format の重複定義を統合する
  - @ユーザー名
```

## 解決方法

- `src/device_windows.rs` に `pub(crate) unsafe fn determine_audio_format()` を追加した
- `src/capture_windows.rs` の同名関数と `src/playback_windows.rs` の `determine_playback_format()` を削除し、それぞれ `crate::device_windows::determine_audio_format()` を呼ぶように変更した
- 不要になった import (`Win32::Media::KernelStreaming::*`, `Win32::Media::Multimedia::*`) を両ファイルから削除した
- `CHANGES.md` にエントリを追記した
