# プラットフォーム間の共通型を共通モジュールに抽出する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

change

## 概要

`src/device.rs` / `src/device_windows.rs` 間、`src/capture.rs` / `src/capture_windows.rs` 間、`src/playback.rs` / `src/playback_windows.rs` 間で、同一の型定義やメソッド実装が完全に重複している。プラットフォーム非依存のデータ型を `src/common.rs` 等の共通モジュールに抽出し、各プラットフォームファイルには実際のデバイス操作ロジックのみを残す。

## 対象の重複

- `AudioDeviceType` / `AudioFormat`: `src/device.rs:8-32` と `src/device_windows.rs:27-42` で完全に同一
- `AudioDevice` の公開メソッド群 (`name()`, `unique_id()`, `channels()`, `sample_rate()`, `device_type()`): `src/device.rs:48-78` と `src/device_windows.rs:53-73` で同一シグネチャ
- `AudioDeviceList` の公開メソッド群 (`devices()`, `len()`, `is_empty()`): `src/device.rs:139-150` と `src/device_windows.rs:105-116` で同一シグネチャ
- `AudioFrame` / `AudioFrameOwned` の全実装: `src/capture.rs:11-123` と `src/capture_windows.rs:17-130` で完全に同一
- `AudioCaptureConfig`: `src/capture.rs:125-139` と `src/capture_windows.rs:132-146` で完全に同一
- `CaptureContext`: `src/capture.rs:141-144` と `src/capture_windows.rs:148-151` で完全に同一
- `PlaybackFrame` と `AudioPlaybackConfig`: `src/playback.rs:9-79` と `src/playback_windows.rs:17-88` で完全に同一

## 根拠

DRY 原則に対する重大な違反。修正時に 2 箇所の変更が必要になり、片方だけ更新された場合にコンパイルエラーにならず動作が分岐する危険性がある。また PBT がテスト可能な対象が分散しており、テストの記述も煩雑になる。

## 対応方針

1. `src/common.rs` を作成し、以下の型を移動する
   - `AudioDeviceType`
   - `AudioFormat`
   - `AudioFrame`
   - `AudioFrameOwned`
   - `AudioCaptureConfig`
   - `PlaybackFrame`
   - `AudioPlaybackConfig`
   - `CaptureContext` / `PlaybackContext`
2. `src/lib.rs` からの re-export を共通モジュール経由に変更する
3. `src/device.rs` / `src/device_windows.rs` から重複型定義を削除し、`use crate::common::*` に置き換える
4. `src/capture.rs` / `src/capture_windows.rs` から重複型定義を削除し、`use crate::common::*` に置き換える
5. `src/playback.rs` / `src/playback_windows.rs` から重複型定義を削除し、`use crate::common::*` に置き換える

## 注意点

- `AudioDevice` 型は内部構造がプラットフォーム間で異なるため共通化は難しいが、`AudioDeviceType` / `AudioFormat` は共通化可能
- `CaptureContext` / `PlaybackContext` はプラットフォーム非依存のコールバック保持構造体であり共通化可能
- `AudioFrame` の `as_s16()` / `as_f32()` 実装は共通の `src/common.rs` に移動可能
- `PlaybackFrame::from_s16()` / `from_f32()` はプラットフォーム非依存なので共通化可能
