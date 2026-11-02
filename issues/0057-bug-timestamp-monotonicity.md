# CoreAudio のタイムスタンプ 0 フォールバックを修正し契約を明文化する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-timestamp-monotonicity
- Polished: {YYYY-MM-DD}

## 目的

CoreAudio がホスト時刻を取得できない場合に `timestamp_us = 0` を返すことで単調性が壊れる問題を修正し、`timestamp_us` の契約を明文化する。

## 現状

- `audio_coreaudio.m` の `audio_input_callback` は `start_time->mFlags` に `kAudioTimeStampHostTimeValid` が無い場合、`timestamp_us` を 0 のままコールバックへ渡す。
- 他のバックエンドは PulseAudio が `CLOCK_MONOTONIC - latency`、PipeWire が `pw_buffer.time` / `pw_stream_get_nsec`、WASAPI が QPC を基点とする単調時計を使っており、0 は他のどのパケットとも比較できない値である。0 が混入するとタイムスタンプの単調増加も壊れる。
- `AudioFrame.timestamp_us` と `audio.h` の共通仕様には、単位 (マイクロ秒) 以外に基準時刻・単調性・バックエンド依存性が書かれていない。利用者が壁時計と比較したり、バックエンド間で比較したりする誤用を招く。
- `tests/test_device_timing.rs` は単調増加を前提に検証している。

## 設計方針

- CoreAudio の 0 フォールバックを、直前のタイムスタンプ + 音声時間、または `mach_absolute_time` 系の単調時計へ置き換える。
- `audio.h` と `AudioFrame` / `AudioFrameOwned` の doc に「同一セッション内で単調増加するマイクロ秒値。基準時刻はバックエンド固有で、壁時計でもバックエンド間で比較可能でもない」と明記する。
- 0 や非単調値が発生しないことを検証するテストを追加する。

## 完了条件

- CoreAudio のキャプチャで `timestamp_us` が 0 にならない。
- 同一セッション内でタイムスタンプが単調増加する。
- 契約が rustdoc と `audio.h` に書かれている。
- テストが追加され、`cargo test --workspace` が通る。
