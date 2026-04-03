# SendPtr<T> の blanket Send impl が無条件すぎる

Created: 2026-04-03
Model: Opus 4.6

## 概要

`capture_windows.rs` と `playback_windows.rs` の両方で定義されている `SendPtr<T>` に対して `unsafe impl<T> Send for SendPtr<T> {}` が無条件に付与されている。任意の型に `Send` を付与できてしまい、COM オブジェクトのスレッドモデルの安全性の根拠がコード上にない。

## 根拠

`windows` crate は COM インターフェースに意図的に `Send` を実装していない場合がある。`SendPtr<T>` の blanket impl はこの制約を完全にバイパスしている。現在は MTA (`COINIT_MULTITHREADED`) で初期化しており、使用している `IAudioClient` / `IAudioCaptureClient` / `IAudioRenderClient` は MTA 前提で安全にスレッド間移送できるが、その根拠がコードに記載されていない。また、将来的に他の COM 型を `SendPtr` で包んだ場合に安全性が担保されない。

## 該当箇所

- `src/capture_windows.rs:171-172`
- `src/playback_windows.rs:114-115`

## 修正方針

blanket impl をやめて、使用している具体型のみに `Send` を付与する。安全性の根拠を safety コメントとして記載する。

## 解決方法

Completed: 2026-04-03

`capture_windows.rs` では `SendPtr<IAudioCaptureClient>` のみ、`playback_windows.rs` では `SendPtr<IAudioRenderClient>` と `SendPtr<IAudioClient>` のみに `Send` を実装するように変更した。MTA 前提で安全にスレッド間移送できる旨の safety コメントも追加した。
