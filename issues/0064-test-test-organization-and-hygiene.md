# テストの構成とテストコードの品質を整理する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-test-organization-and-hygiene
- Polished: {YYYY-MM-DD}

## 目的

テストファイルの命名・分割・重複・誤解を招くテスト名を整理し、テストコードの保守性を上げる。

## 現状

- `tests/test_device_timing.rs` は `src/device_timing.rs` に対応しない横断テストなのに `test_` プレフィックスを持ち、`tests/session_lifecycle.rs` と扱いが不統一である。
- `tests/test_device_timing.rs` は 576 行で `mod` 分割が無く、デバイス不要の純粋関数テスト (`estimate_frame_rate` / `is_measurement_sufficient` など) が `#![cfg(enable_default_*)]` に巻き込まれている。`--no-default-features` や明示バックエンドのみの構成ではコンパイルされず、純粋関数のテストが実行されない。
- `DEVICE_TEST_LOCK` はテストが panic すると poisoned になり、もう一方のテストが原因と無関係なメッセージで失敗する。
- `tests/test_device_timing.rs` の `enumerate_input_devices` / `enumerate_output_devices` は空判定のためだけに `AudioDeviceList` を返し、呼び出し側が即 `drop` している。
- `tests/test_capture.rs` と `tests/test_playback.rs` の `accepts_callback_*` はほぼ同一のコンパイル確認テストが重複しており、結果を `let _ =` で捨てるため実行時の受理は検証していない。
- `src/playback_ffi.rs` の `fills_buffer_with_silence_when_callback_returns_none` はバッファを検証しておらず、名前と検証内容が一致しない。`returns_zero_when_buffer_size_overflows` も、64 bit では i32 入力でこの乗算が `usize` を超えないため、実際にはオーバーフロー経路を通っていない。
- `tests/test_error.rs` の Display 検証は部分文字列の `contains` のみで、規約が想定する insta のスナップショットになっていない。
- `AudioDeviceType::from_ffi` / `AudioFormat::from_ffi` のテストが `src/device_ffi.rs` にあり、定義元の `src/common.rs` と対応していない。
- `tests/session_lifecycle.rs` の 4 テストは同じ実機デバイスを並列に開き得る。`test_device_timing` には直列化ロックがあるが、こちらには無い。
- `tests/session_lifecycle.rs` の `stop_while_callback_blocked` の引数名 `release` が実態と合わず、サイクル数の偶奇に依存したコメントも残っている。

## 設計方針

- `tests/test_device_timing.rs` を横断テストとして `tests/device_timing.rs` にリネームし、純粋関数テストは `mod` 分割するか、cfg の外へ出す。
- Mutex は `unwrap_or_else(|e| e.into_inner())` で poison を回復する。実機デバイスを使うテスト全体で共有するロックを検討する。
- 重複するコンパイル確認テストは共有ヘルパーへ寄せるか、片方に残す理由をコメントする。テスト名を検証内容に合わせる。
- Display 検証は insta のスナップショットに置き換える。
- `from_ffi` のテストは `src/common.rs` の `#[cfg(test)]` へ移す。
- コメント・引数名・コメントの前提を実装に合わせる。

## 完了条件

- テストファイル名と分割が規約に沿っている。
- 純粋関数テストが feature 構成に関係なく実行される。
- 重複テストと誤解を招くテスト名が解消されている。
- Mutex の poison と実機デバイスの並列アクセスが考慮されている。
- `cargo test --workspace` が通る。
