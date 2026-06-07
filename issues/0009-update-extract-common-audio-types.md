# プラットフォーム間の共通型を共通モジュールに抽出する

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

update

## 概要

`src/device.rs` / `src/device_windows.rs` 間、`src/capture.rs` / `src/capture_windows.rs` 間、`src/playback.rs` / `src/playback_windows.rs` 間で、同一の型定義やメソッド実装が完全に重複している。プラットフォーム非依存のデータ型を `src/common.rs` に抽出し、各プラットフォームファイルには実際のデバイス操作ロジックのみを残す。

## 対象の重複

- `AudioDeviceType` / `AudioFormat` の enum 定義本体: `src/device.rs:7-14, 25-32` と `src/device_windows.rs:26-33, 35-42` で同一
  - 注意: `AudioDeviceType::from_ffi()` と `AudioFormat::from_ffi()` は `device.rs` にのみ存在し `crate::ffi` に依存するため、`common.rs` には移動しない。各プラットフォームファイルの `impl` ブロックとして残す。
- `AudioFrame` / `AudioFrameOwned` の全実装: `src/capture.rs` と `src/capture_windows.rs` で完全に同一
- `AudioCaptureConfig`: `src/capture.rs` と `src/capture_windows.rs` で完全に同一
- `CaptureContext` 構造体: `src/capture.rs` と `src/capture_windows.rs` で完全に同一（private なので抽出時に `pub(crate)` に変更する）
- `PlaybackFrame` と `AudioPlaybackConfig`: `src/playback.rs` と `src/playback_windows.rs` で完全に同一

## 非対象（共通化しない）

- `AudioDevice` 構造体とそのメソッド群: 内部構造がプラットフォーム間で異なる（Unix は `NonNull<ffi::AudioDevice>`、Windows は各フィールドを直接保持）ため共通化しない
- `AudioDeviceList` 構造体とそのメソッド群: 内部構造・`Drop` 実装がプラットフォーム間で異なるため共通化しない
- `PlaybackContext` 構造体: `playback_windows.rs` にのみ存在し、`playback.rs`(Unix) には存在しないため重複解消の対象ではない。本 issue では `playback_windows.rs` に留める
- `SendHandle` / `SendPtr<T>` (Windows): `capture_windows.rs` と `playback_windows.rs` で重複しているが、Windows 専用の COM newtype であり本 issue の共通化対象からは除外する。別 issue (0020) で対応する

## 根拠

DRY 原則に対する違反。`#[cfg]` で相互排他なプラットフォーム別ファイルを採用しているため、PR レビュー時に片方のファイルで修正漏れがあっても同一ビルドではコンパイルエラーにならず、別プラットフォームでのみ発覚する。結果として CI の全プラットフォームビルドが通るまで検出されず、修正コストが増大する。また PBT がテスト対象を import する際にプラットフォームごとに異なるパスを参照する必要があり、テスト記述が煩雑になる。共通モジュールに抽出することでメンテナンス性を向上させ、PBT の記述も簡潔にする。

## テスト戦略

- 既存の `pbt/tests/prop_capture.rs` と `pbt/tests/prop_playback.rs` は crate root からの re-export を import しているため、re-export 経路が維持される限りコード変更不要
- 抽出後に全テストがコンパイル通過し全件パスすることを確認する
- `common.rs` 自体には単体テストを配置しない（データ型の定義のみであり、テストすべきロジックを含まないため）

## 後方互換性

`src/lib.rs` の `pub use` の re-export 先を変更するが、外部から利用する公開 API パス（例: `shiguredo_audio_device::AudioDeviceType`）は一切変更しない。したがって外部利用者に対する後方互換性は完全に維持される。

## 対応方針

### 1. `src/common.rs` を作成し、以下の型を移動する

以下の型を `src/common.rs` に定義する:

- `AudioDeviceType` (pub enum { Input, Output }) — derive は `Debug, Clone, Copy, PartialEq, Eq`
- `AudioFormat` (pub enum { S16, F32 }) — derive は `Debug, Clone, Copy, PartialEq, Eq`
- `AudioFrame<'a>` (pub struct { data, frames, channels, sample_rate, format: AudioFormat, timestamp_us }) とその `impl` ブロック (`to_owned()`, `as_s16()`, `as_f32()`)
- `AudioFrameOwned` (pub struct + `Debug, Clone, PartialEq, Eq` derive) とその `impl` ブロック (`as_frame()`, `as_s16()`, `as_f32()`)
- `AudioCaptureConfig` (pub struct { device_id: Option<String>, sample_rate: i32, channels: i32 }) と `impl Default`
- `PlaybackFrame` (pub struct { data: Vec<u8>, frames, channels, sample_rate, format: AudioFormat }) とその `impl` ブロック (`from_s16()`, `from_f32()`)
- `AudioPlaybackConfig` (pub struct { device_id: Option<String>, sample_rate: i32, channels: i32 }) と `impl Default`
- `CaptureContext` — `pub(crate) struct { callback: Box<dyn Fn(AudioFrame<'_>) + Send + Sync>, running: AtomicBool }`

`common.rs` の必要 import:
- `use std::sync::atomic::AtomicBool;`
- `use crate::error::{Error, Result};`

### 2. `src/lib.rs` に `mod common;` を追加し、re-export を再構築する

変更前（現在の lib.rs）:
```rust
mod error;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod capture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod device;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod ffi;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod playback;

#[cfg(target_os = "windows")]
mod capture_windows;
#[cfg(target_os = "windows")]
mod device_windows;
#[cfg(target_os = "windows")]
mod playback_windows;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use capture::{AudioCapture, AudioCaptureConfig, AudioFrame, AudioFrameOwned};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use device::{AudioDevice, AudioDeviceList, AudioDeviceType, AudioFormat};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use playback::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

#[cfg(target_os = "windows")]
pub use capture_windows::{AudioCapture, AudioCaptureConfig, AudioFrame, AudioFrameOwned};
#[cfg(target_os = "windows")]
pub use device_windows::{AudioDevice, AudioDeviceList, AudioDeviceType, AudioFormat};
#[cfg(target_os = "windows")]
pub use playback_windows::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

pub use error::{Error, Result};
```

変更後:
```rust
mod error;
mod common;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod capture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod device;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod ffi;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod playback;

#[cfg(target_os = "windows")]
mod capture_windows;
#[cfg(target_os = "windows")]
mod device_windows;
#[cfg(target_os = "windows")]
mod playback_windows;

pub use common::{
    AudioCaptureConfig, AudioDeviceType, AudioFormat, AudioFrame, AudioFrameOwned,
    AudioPlaybackConfig, PlaybackFrame,
};

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use capture::AudioCapture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use device::{AudioDevice, AudioDeviceList};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use playback::AudioPlayback;

#[cfg(target_os = "windows")]
pub use capture_windows::AudioCapture;
#[cfg(target_os = "windows")]
pub use device_windows::{AudioDevice, AudioDeviceList};
#[cfg(target_os = "windows")]
pub use playback_windows::AudioPlayback;

pub use error::{Error, Result};
```

### 3. 各プラットフォームファイルの変更

#### `src/device.rs` の変更

- `AudioDeviceType` と `AudioFormat` の enum 定義（`pub enum AudioDeviceType { ... }` と `pub enum AudioFormat { ... }` の本体）のみを削除する
- `from_ffi()` impl ブロックは削除せず `device.rs` に残す（型が `common` から import されるため、そのまま `impl AudioDeviceType`、`impl AudioFormat` として有効）
- `use crate::common::{AudioDeviceType, AudioFormat};` を追加する
- `use crate::ffi;` は維持する

#### `src/device_windows.rs` の変更

- `AudioDeviceType` と `AudioFormat` の enum 定義を削除する
- `use crate::common::{AudioDeviceType, AudioFormat};` を追加する

#### `src/capture.rs` の変更

- `AudioFrame`, `AudioFrameOwned`, `AudioCaptureConfig`, `CaptureContext` の定義と `impl` を削除する
- `use crate::device::AudioFormat;` を `use crate::common::{AudioFormat, AudioFrame, AudioFrameOwned, AudioCaptureConfig, CaptureContext};` に置き換える
- `use crate::ffi;` は `capture.rs` 内で `AudioFormat::from_ffi()` を呼んでいるため維持する（`from_ffi` は `device.rs` の `impl AudioFormat` に定義され、同一クレート内で解決される）

#### `src/capture_windows.rs` の変更

- `AudioFrame`, `AudioFrameOwned`, `AudioCaptureConfig`, `CaptureContext` の定義と `impl` を削除する
- 既存の import を以下のように書き換える:
  ```rust
  // 変更前
  use crate::device_windows::{AudioDeviceType, AudioFormat, get_device_by_id};
  // 変更後
  use crate::device_windows::get_device_by_id;
  use crate::common::{AudioDeviceType, AudioFormat, AudioFrame, AudioFrameOwned, AudioCaptureConfig, CaptureContext};
  ```

#### `src/playback.rs` の変更

- `PlaybackFrame`, `AudioPlaybackConfig` の定義と `impl` を削除する
- `use crate::device::AudioFormat;` を `use crate::common::{AudioPlaybackConfig, PlaybackFrame};` に置き換える

#### `src/playback_windows.rs` の変更

- `PlaybackFrame`, `AudioPlaybackConfig` の定義と `impl` を削除する
- 既存の import を以下のように書き換える:
  ```rust
  // 変更前
  use crate::device_windows::{AudioDeviceType, AudioFormat, get_device_by_id};
  // 変更後
  use crate::device_windows::get_device_by_id;
  use crate::common::{AudioDeviceType, AudioFormat, AudioPlaybackConfig, PlaybackFrame};
  ```
- `PlaybackContext` は変更しない（`playback_windows.rs` に留める）

### 4. CHANGES.md への追記

`## develop` セクションの `### misc` サブセクションに以下のエントリを追記する:

```
- [UPDATE] プラットフォーム間で重複していた共通型を src/common.rs に抽出する
  - @ユーザー名
```

## 対象外の重複

以下の重複は本 issue では対応せず、別 issue で対応する:

- `SendHandle` / `SendPtr<T>` 型: `capture_windows.rs` と `playback_windows.rs` で重複。Windows 専用 COM newtype のため別 issue (0020) で対応する
- `determine_audio_format()` / `determine_playback_format()`: `capture_windows.rs` と `playback_windows.rs` で類似実装。別 issue (0019) で対応する

## 他 issue との依存関係

- **0019** (fix-deduplicate-audio-format-functions) と **0020** (fix-deduplicate-send-handle) はいずれも `capture_windows.rs` / `playback_windows.rs` を編集対象としており、0009 がこれらのファイルの import 構成を大きく変更する。そのため 0009 を 0019/0020 より先に適用する必要がある（逆順だと conflict が発生する）

## 実装時の注意

- `common.rs` に移動する型定義を各プラットフォームファイルから削除した後、`use std::sync::atomic::{AtomicBool, Ordering};` の import が未使用になるファイルがある。`AtomicBool` は `CaptureContext` のフィールド型として使われていたが、移動後は各ファイルで直接参照されなくなる。`Ordering` のみを残す import に縮小するか、unused import 警告を抑制する
- 移動後、全プラットフォーム（macOS, Linux, Windows）でビルドが通過することを確認する
- 既存 PBT テスト (`pbt/tests/prop_capture.rs`, `pbt/tests/prop_playback.rs`) のコード変更は不要だが、テストが全件パスすることを確認する
- `update` カテゴリのブランチ prefix は CLAUDE.md に明示的に定義されていないため、本プロジェクトでは `feature/fix-`、`feature/add-`、`feature/change-` のうち後方互換がある変更であることから `feature/update-` を慣例として使用する
