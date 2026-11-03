# Error のトレイト実装と公開型の derive を整備する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-error-traits-and-derives
- Polished: 2026-11-04

## 目的

`Error` の `PartialEq` / `Eq` 実装と公開型の derive の不揃いを解消し、利用側の比較・表示・保存をしやすくする。

## 現状

- `src/error.rs` の `Error` は `Debug` / `Clone` / `Copy` のみを derive しており、`PartialEq` / `Eq` が無い。テストは `matches!` で比較している。
- `src/error.rs` の `impl std::error::Error for Error {}` は `source()` を実装しておらず、`Error::DataTooLarge(TryFromIntError)` の内部エラーが失われる。`pbt/tests/prop_error.rs` の `prop_error_source_is_none` が「常に None」を仕様として固定している。この `source()` の実装と PBT の期待値変更は `issues/0066-test-improve-pbt-and-fuzzing.md` で扱う。
- `src/common.rs` の `AudioFrame` に `Debug` が無い。`PlaybackFrame` は `Debug` / `Clone` のみで比較できない。`AudioCaptureConfig` / `AudioPlaybackConfig` は derive が無く、`AudioCapture::config` / `AudioPlayback::config` が `&Config` を返すのに `Clone` が無いため利用側が設定を保存できない。
- `src/capture.rs` / `src/playback.rs` / `src/device.rs` の `AudioCapture` / `AudioPlayback` / `AudioDevice` / `AudioDeviceList` と各セッション型も `Debug` を持たない。
- `src/runtime.rs` の `ThreadGuard` も `Debug` を持たないが、初期化ガードであり本 issue の対象外とする。

## 設計方針

- `src/error.rs` の `Error` に `PartialEq` / `Eq` を追加する。`TryFromIntError` は Rust 1.95 で `PartialEq` / `Eq` を実装済みである。`source()` の実装と `prop_error_source_is_none` の期待値変更は `issues/0066-test-improve-pbt-and-fuzzing.md` の範囲とし、本 issue では扱わない。
- `src/common.rs` の `AudioFrame` に `Debug` を追加する。`AudioFrame` は内部に FFI ポインタを持たないため derive する。
- `src/common.rs` の `PlaybackFrame` に `PartialEq` / `Eq` を追加する。`Vec<u8>` と `AudioFormat` は `Eq` を実装しているため derive する。
- `src/common.rs` の `AudioCaptureConfig` / `AudioPlaybackConfig` に `Debug` / `Clone` を追加する。`device_id: Option<String>` は `Clone` / `Debug` を実装しているため derive する。
- `src/capture.rs` の `AudioCapture` / `AudioCaptureSession`、`src/playback.rs` の `AudioPlayback` / `AudioPlaybackSession`、`src/device.rs` の `AudioDevice` / `AudioDeviceList` に `Debug` を追加する。`FfiCaptureImpl` / `FfiPlaybackImpl` / `FfiDeviceImpl` / `FfiDeviceListImpl` は FFI ポインタやユーザーコールバックを含み derive できないため、公開アクセサ (`config` / `device_type` / `name` / `unique_id` / `as_slice`) を使った手実装にする。`Debug` 出力に内部の FFI ポインタやコールバックを含めない。
- 公開 API の追加なので、`CHANGES.md` の `## develop` に `[ADD]` エントリを追加する。
- `tests/test_error.rs` に `Error` の `PartialEq` / `Eq` を使った比較テストを、`tests/test_common.rs` に `AudioFrame` の `Debug`、`PlaybackFrame` の `PartialEq` / `Eq`、両 Config の `Debug` / `Clone` のテストを追加する。`AudioCapture` / `AudioPlayback` / `AudioDevice` / `AudioDeviceList` と各セッション型の `Debug` は、既存のテストで `format!("{:?}", ...)` を検証する。

## 完了条件

- `Error` の比較が `assert_eq!` でできる。
- `AudioFrame` が `Debug` を実装している。
- `PlaybackFrame` が `PartialEq` / `Eq` を実装し、比較できる。
- `AudioCaptureConfig` / `AudioPlaybackConfig` が `Debug` / `Clone` を実装している。
- `AudioCapture` / `AudioCaptureSession` / `AudioPlayback` / `AudioPlaybackSession` / `AudioDevice` / `AudioDeviceList` が `Debug` を実装し、その出力に内部の FFI ポインタやコールバックが含まれない。
- 追加したトレイトを検証する単体テストが更新され、`cargo test --workspace` が通る。
- `CHANGES.md` の `## develop` に `[ADD]` エントリが追加されている。
