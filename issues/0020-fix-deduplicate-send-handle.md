# SendHandle / SendPtr の重複定義を共通化する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

fix

## 概要

`capture_windows.rs` と `playback_windows.rs` で、`SendHandle` と `SendPtr<T>` の newtype ラッパーが別々に定義されている。

## 対象箇所

- `src/capture_windows.rs:163-181` — `SendHandle` / `SendPtr<IAudioCaptureClient>`
- `src/playback_windows.rs:106-126` — `SendHandle` / `SendPtr<IAudioRenderClient>` / `SendPtr<IAudioClient>`

```rust
// capture_windows.rs
struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}
impl SendHandle {
    fn into_inner(self) -> HANDLE { self.0 }
}

struct SendPtr<T>(T);
unsafe impl Send for SendPtr<IAudioCaptureClient> {}
impl<T> SendPtr<T> {
    fn into_inner(self) -> T { self.0 }
}

// playback_windows.rs に同一コードが重複
```

## 根拠

`SendHandle` は `HANDLE` を `Send` にするための汎用ラッパーであり、モジュール間で重複する理由はない。`SendPtr<T>` も同様に汎用的なラッパーである。DRY 原則違反であり、将来的な修正時に片方だけ更新されるリスクがある。

## 対応方針

`SendHandle` と `SendPtr<T>` を共通モジュール（例: `src/device_windows.rs` に `pub(crate)` として）に移動し、1 か所で定義する。各 COM インターフェース型に対する `unsafe impl Send` は利用側で宣言する。
