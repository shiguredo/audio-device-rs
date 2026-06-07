# SendHandle / SendPtr の重複定義を共通化する

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## 概要

`capture_windows.rs` と `playback_windows.rs` で、`SendHandle` と `SendPtr<T>` の newtype ラッパーが別々に定義されている。

## 対象箇所

- `src/capture_windows.rs:163-181` — `SendHandle` / `SendPtr<IAudioCaptureClient>`
- `src/playback_windows.rs:106-126` — `SendHandle` / `SendPtr<IAudioRenderClient>` / `SendPtr<IAudioClient>`

## 根拠

`SendHandle` は `HANDLE` を `Send` にするための汎用ラッパーであり、モジュール間で重複する理由はない。`SendPtr<T>` も同様に汎用的なラッパーである。DRY 原則違反であり、将来的な修正時に片方だけ更新されるリスクがある。

## 対応方針

### 1. `src/device_windows.rs` に統合する

`src/device_windows.rs` に `pub(crate)` として共通定義を移動する:

```rust
// src/device_windows.rs に追加

/// Send でない型をスレッドに渡すためのラッパー（MTA で初期化済みのため安全）
pub(crate) struct SendHandle(pub(crate) HANDLE);
unsafe impl Send for SendHandle {}
impl SendHandle {
    pub(crate) fn into_inner(self) -> HANDLE {
        self.0
    }
}

pub(crate) struct SendPtr<T>(pub(crate) T);

// Safety: COM オブジェクトは MTA (COINIT_MULTITHREADED) で初期化しており、
// MTA オブジェクトはスレッド間で安全に移送できる。
// 各具体型に対する unsafe impl Send は、利用側（capture_windows.rs, playback_windows.rs）で宣言する。

impl<T> SendPtr<T> {
    pub(crate) fn into_inner(self) -> T {
        self.0
    }
}
```

### 2. 各ファイルの変更

#### `capture_windows.rs`

- `SendHandle`, `SendPtr<T>` の定義を削除する
- `unsafe impl Send for SendPtr<IAudioCaptureClient> {}` のみを残す

#### `playback_windows.rs`

- `SendHandle`, `SendPtr<T>` の定義を削除する
- `unsafe impl Send for SendPtr<IAudioRenderClient> {}` と `unsafe impl Send for SendPtr<IAudioClient> {}` のみを残す

### 3. 注意点

- `SendPtr` のフィールド `0` は `pub(crate)` にして `device_windows.rs` に移動する。`SendHandle` のフィールドも同様に `pub(crate)` にする
- 両ファイルで `use crate::device_windows::{SendHandle, SendPtr};` を追加する

## CHANGES.md への追記

`## develop` セクションの `### misc` サブセクションに以下のエントリを追記する:

```
- [FIX] SendHandle / SendPtr の重複定義を共通化する
  - @ユーザー名
```
