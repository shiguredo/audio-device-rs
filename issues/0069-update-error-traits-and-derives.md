# Error のトレイト実装と公開型の derive を整備する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-error-traits-and-derives
- Polished: {YYYY-MM-DD}

## 目的

`Error` の標準トレイト実装と公開型の derive の不揃いを解消し、利用側の比較・表示・保存をしやすくする。

## 現状

- `Error` は `Debug` / `Clone` / `Copy` のみを derive しており、`PartialEq` / `Eq` が無い。テストは `matches!` で比較している。
- `impl std::error::Error for Error {}` は `source()` を実装しておらず、`Error::DataTooLarge(TryFromIntError)` の内部エラーが失われる。`prop_error_source_is_none` が「常に None」を仕様として固定している。
- `AudioFrame` に `Debug` が無い。`PlaybackFrame` は `Debug` / `Clone` のみで比較できない。`AudioCaptureConfig` / `AudioPlaybackConfig` は derive が一切無く、`config()` が `&Config` を返すのに `Clone` が無いため利用側が設定を保存できない。
- `AudioCapture` / `AudioPlayback` / `AudioDevice` / `AudioDeviceList` と各セッション型も `Debug` を持たない。

## 設計方針

- `Error` に `PartialEq` / `Eq` を追加する (`TryFromIntError` は `PartialEq` / `Eq` を実装済み)。
- `DataTooLarge` のみ `source()` が内部エラーを返すよう実装し、PBT を「DataTooLarge だけ Some、他は None」に変更する。
- `AudioFrame` に `Debug`、`PlaybackFrame` に `PartialEq` / `Eq`、両 Config に `Debug` / `Clone` を追加する。主要型の `Debug` は内部の FFI ポインタを出さない実装にするか、derive 可能な範囲で追加する。

## 完了条件

- `Error` の比較が `assert_eq!` でできる。
- `source()` が `DataTooLarge` の内部エラーを返す。
- 公開型の derive が用途に応じて揃っている。
- PBT と単体テストが更新され、`cargo test --workspace` が通る。
