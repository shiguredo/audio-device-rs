# テストの構成とテストコードの品質を整理する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-test-organization-and-hygiene
- Polished: 2026-11-04

## 目的

テストファイルの命名・分割・重複・誤解を招くテスト名を整理し、テストコードの保守性を上げる。

## 現状

- `tests/test_device_timing.rs` は対応する `src/device_timing.rs` が存在しない横断テストなのに `test_` プレフィックスを持ち、`tests/session_lifecycle.rs` と扱いが不統一である。
- `tests/test_device_timing.rs` は 576 行で `mod` 分割が無く、デバイス不要の純粋関数テスト (`estimate_frame_rate` / `is_measurement_sufficient` など) が `#![cfg(enable_default_*)]` に巻き込まれている。`--no-default-features` や明示バックエンドのみの構成ではコンパイルされず、純粋関数のテストが実行されない。
- `DEVICE_TEST_LOCK` はテストが panic すると poisoned になり、もう一方のテストが原因と無関係なメッセージで失敗する。
- `tests/test_device_timing.rs` の `enumerate_input_devices` / `enumerate_output_devices` は空判定のためだけに `AudioDeviceList` を返し、呼び出し側が即 `drop` している。
- `tests/test_capture.rs` と `tests/test_playback.rs` の `accepts_callback_*` はほぼ同一のコンパイル確認テストが重複しており、結果を `let _ =` で捨てるため実行時の受理は検証していない。
- `src/playback_ffi.rs` の `fills_buffer_with_silence_when_callback_returns_none` はバッファを検証しておらず、名前と検証内容が一致しない。`returns_zero_when_buffer_size_overflows` も、64 bit では i32 入力でこの乗算が `usize` を超えないため、実際にはオーバーフロー経路を通っていない。
- `tests/test_error.rs` の Display 検証は部分文字列の `contains` のみで、規約が想定する insta のスナップショットになっていない。
- `AudioDeviceType::from_ffi` / `AudioFormat::from_ffi` のテストが `src/device_ffi.rs` にあり、定義元の `src/common.rs` と対応していない。
- `tests/session_lifecycle.rs` の 4 テストは同じ実機デバイスを並列に開き得る。`test_device_timing` には直列化ロックがあるが、こちらには無い。
- `tests/session_lifecycle.rs` の `stop_while_callback_blocked` の引数名 `release` はコールバックの送信完了を待つ受信側であり、コメント内の `release.send` も実態と合わない。サイクル数の偶奇に依存したコメントも残っている。

## 設計方針

- `tests/test_device_timing.rs` を横断テストとして `tests/device_timing.rs` にリネームする。ファイル先頭の `#![cfg(...)]` を外し、デバイスを使うテストとヘルパーを `#[cfg(any(enable_default_coreaudio, enable_default_pulse, enable_default_pipewire, enable_default_wasapi))]` 付きの `mod` にまとめる。純粋関数とそのテストは cfg なしの `mod` に分け、feature 構成に関係なくコンパイル・実行できるようにする。
- `tests/device_timing.rs` の `enumerate_input_devices` / `enumerate_output_devices` は、空判定だけを行う `bool` を返すヘルパーに置き換え、呼び出し側で `AudioDeviceList` を即 `drop` しないようにする。
- `DEVICE_TEST_LOCK` は `unwrap_or_else(|e| e.into_inner())` で poison を回復する。`tests/session_lifecycle.rs` にも `static Mutex<()>` を追加し、4 テストを直列化する。ロックはテスト実行の排他制御であり、共有データの保護ではないことをコメントに書く。テストバイナリをまたぐ共有は不要である。
- `tests/test_capture.rs` と `tests/test_playback.rs` の `accepts_callback_*` は共有ヘルパーへ寄せるか、片方に残す理由をコメントする。
- `src/playback_ffi.rs` の `fills_buffer_with_silence_when_callback_returns_none` は、実際の検証内容に合わせた名前に変更する。`returns_zero_when_buffer_size_overflows` も、64 bit でオーバーフローしないことを踏まえた名前に変更する。
- `tests/test_error.rs` の Display 検証は、`insta` を dev-dependencies に追加し、インラインスナップショット (`assert_snapshot!`) に置き換える。`tests/test_common.rs` の `try_from_int_error_display_contains_data_too_large` も、`tests/test_error.rs` へ移動する前後を問わずスナップショットに置き換え、スナップショット検証に合わせた名前に変更する。
- `AudioDeviceType::from_ffi` / `AudioFormat::from_ffi` のテストは `src/common.rs` の `#[cfg(all(test, any(enable_coreaudio, enable_pulse, enable_pipewire)))]` モジュールへ移す。WASAPI のみの構成では `from_ffi` が存在しないため、`enable_wasapi` だけの構成でコンパイルしないようにする。
- `tests/session_lifecycle.rs` の `stop_while_callback_blocked` の引数名を、コールバックの送信完了を待つ受信側であることが分かる名前に変更する。`release.send` というコメントと、サイクル数の偶奇に依存したコメントを実装に合わせる。

## 完了条件

- テストファイル名と分割が規約に沿っている。
- 純粋関数テストが feature 構成に関係なく実行される。`cargo test --no-default-features` で実行できることを確認する。
- 重複テストと誤解を招くテスト名が解消されている。
- 列挙ヘルパーが `AudioDeviceList` を返さず、空判定だけを行う。
- `DEVICE_TEST_LOCK` と `tests/session_lifecycle.rs` のロックが poison を回復し、`tests/session_lifecycle.rs` の 4 テストが直列化されている。
- `tests/test_error.rs` の Display 検証がインラインスナップショットになっている。
- `try_from_int_error_display_contains_data_too_large` に相当するテストが、`tests/test_common.rs` または `tests/test_error.rs` のどちらにあってもスナップショット検証になっている。
- `AudioDeviceType::from_ffi` / `AudioFormat::from_ffi` のテストが `src/common.rs` にあり、WASAPI のみの構成でもコンパイルできる。
- コメント・引数名・コメントの前提が実装と一致している。
- `cargo test --workspace` が通る。
