# Windows コールバックで panic が未捕捉

Created: 2026-04-03
Model: Opus 4.6

## 概要

macOS/Linux のキャプチャコールバックは `std::panic::catch_unwind` で保護されているが、Windows のキャプチャ・再生コールバックでは panic が素通しになっている。ユーザーコールバックが panic するとワーカースレッドが即死し、`join()` の失敗も握り潰されるため、「突然無音になる」形で壊れる。

## 根拠

- macOS/Linux: `src/capture.rs:283` で `catch_unwind` を使用
- Windows キャプチャ: `src/capture_windows.rs:483` で素通し
- Windows 再生: `src/playback_windows.rs:396` で素通し

プラットフォーム間で動作の対称性がなく、Windows のみユーザーコールバックの panic でサイレントに壊れる。

## 修正方針

Windows のキャプチャ・再生両方のコールバック呼び出しを `std::panic::catch_unwind` で囲む。

## 解決方法

Completed: 2026-04-03

`capture_windows.rs` のキャプチャコールバックと `playback_windows.rs` の再生コールバックを `std::panic::catch_unwind(AssertUnwindSafe(...))` で囲んだ。再生側は panic 時に `None` を返すことで無音フレームとして処理される。macOS/Linux 側と同等の保護が得られるようになった。
