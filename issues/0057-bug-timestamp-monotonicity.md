# CoreAudio のタイムスタンプ 0 フォールバックを修正し契約を明文化する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-timestamp-monotonicity
- Polished: 2026-11-03

## 目的

CoreAudio がホスト時刻を取得できない場合に `timestamp_us = 0` を返すことで、セッション途中でタイムスタンプの単調増加が壊れる問題を修正し、`timestamp_us` の契約を明文化する。

## 現状

- `src/audio_coreaudio.m` の `audio_input_callback` は、`start_time->mFlags` に `kAudioTimeStampHostTimeValid` が無い場合、`timestamp_us` を 0 のままコールバックへ渡す。`mach_timebase_info` が失敗した場合も 0 のままになる。
- 他のバックエンドは、`src/audio_pulse.c` の `monotonic_time_us` / `capture_timestamp_us` が `CLOCK_MONOTONIC` を、`src/audio_pipewire.c` の `capture_timestamp_us` / `stream_time_us` が `pw_buffer.time` / `pw_stream_get_nsec` を、`src/capture_wasapi.rs` の `timestamp_for_packet` / `qpc_position_to_micros` が QPC を基点とする単調時計を使っており、0 はキャプチャ時刻を表さない。セッション途中で 0 が混入すると、直前のタイムスタンプより小さくなり単調増加が壊れる。
- `src/common.rs` の `AudioFrame.timestamp_us` / `AudioFrameOwned.timestamp_us` の doc は単位 (マイクロ秒) だけを書き、`src/audio.h` のコールバック共通仕様にはタイムスタンプの記述自体が無い。基準時刻・単調性・バックエンド依存性が書かれておらず、利用者が壁時計と比較したり、バックエンド間で比較したりする誤用を招く。
- `tests/test_device_timing.rs` の `verify_capture_timestamps` は、連続するフレームのタイムスタンプが単調増加し、直前のフレームの音声時間と 1 ms 以内で一致することを検証している。ただし、`timestamp_us` が 0 でないことは明示的に検証していない。

## 設計方針

- `src/audio_coreaudio.m` の `struct AudioSession` に、直前のタイムスタンプ、直前のフレーム数、および直前の値の有無を保持するフィールドを追加する。
- `audio_input_callback` のタイムスタンプ算出を次のとおり変更する。
  - `kAudioTimeStampHostTimeValid` が立ち、`mach_timebase_info` が成功し、`timebase.denom` が 0 でない場合は、`start_time->mHostTime` をマイクロ秒へ換算した値を使う。
  - ホスト時刻が無効な場合、または `mach_timebase_info` が失敗した場合、または `timebase.denom` が 0 の場合は、直前のタイムスタンプに直前のフレーム数の音声時間を加えた値を使う。
  - セッション先頭で直前の値が無い場合は、`mach_absolute_time` をマイクロ秒へ換算した値を使う。`mach_timebase_info` が失敗した場合、または `timebase.denom` が 0 の場合は `clock_gettime(CLOCK_MONOTONIC)` で取得した時刻をマイクロ秒へ換算した値を使う。
  - 直前のタイムスタンプに音声時間を加える計算は、`INT64_MAX` を超える場合は `INT64_MAX` に飽和させる。算出した値が直前のタイムスタンプ以下になる場合は、直前のタイムスタンプ + 1 マイクロ秒にクランプする。`INT64_MAX` に達している場合は `INT64_MAX` を返す。
  - 算出した値が 0 以下の場合は 1 にする。
- `src/audio.h` のコールバック共通仕様と、`src/common.rs` の `AudioFrame.timestamp_us` / `AudioFrameOwned.timestamp_us` の doc に、次の契約を明記する。
  - キャプチャのコールバックで渡される値は、同一セッション内で単調増加するマイクロ秒値であること。
  - 基準時刻はバックエンド固有であり、壁時計ではなく、バックエンド間でも比較できないこと。
- `tests/test_device_timing.rs` の `verify_capture_timestamps` に、各フレームの `timestamp_us` が 0 より大きいことを検証するアサーションを追加する。フォールバック分岐自体は実機でホスト時刻が無効になる状況を再現しにくいため、`audio_input_callback` のコードレビューで確認する。
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加する。

## 完了条件

- CoreAudio のキャプチャで `timestamp_us` が 0 にならない。
- 同一セッション内でタイムスタンプが単調増加する。
- 契約が rustdoc と `audio.h` に書かれている。
- `tests/test_device_timing.rs` に `timestamp_us > 0` の検証が追加され、`cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が通る。
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されている。
