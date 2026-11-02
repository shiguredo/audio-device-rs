# PulseAudio 再生の書き込み長をフレーム境界に揃える

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pulse-playback-write-length
- Polished: {YYYY-MM-DD}

## 目的

PulseAudio 再生で、コールバックが書き込んだ領域と `pa_stream_write` が送る領域が食い違い、末尾に古いデータが残る可能性を解消する。

## 現状

- `audio_pulse.c` の `stream_write_callback` は `pa_stream_begin_write` で `len` バイトのバッファを取得し、`frames = len / frame_size` のフレーム数でユーザーコールバックを呼んだあと、`pa_stream_write(s, buf, len, ...)` で `len` バイト全体を書き込む。
- Rust 側の `write_playback_frame_to_buffer` がゼロ埋めするのは `frames * frame_size` までであり、`len` が `frame_size` の倍数でない場合、末尾の `len - frames * frame_size` バイトは以前のバッファ内容が残ったまま送られる。
- コメントの「written > 0 の場合は Rust 側の共通変換関数がバッファ末尾をゼロ埋めしている」は、この保証を過大に主張している (C バックエンドが Rust の実装に依存しており、`CODEBASE.md` のモジュール責務の原則にも反する)。
- `AudioPlayback` 経由でチャンネル数やサンプルレートの不整合な `PlaybackFrame` が返された場合も、同じ末尾領域の問題が顕在化し得る。

## 設計方針

- `pa_stream_write` に渡す長さを `frames * frame_size` に丸め、残りをゼロ埋めする。または `len` 全体を明示的にゼロ埋めしてから書き込む。
- コメントを「書き込み済みフレーム数までは変換関数が埋め、それ以降は無音として扱う」に修正する。
- `len` が `frame_size` の倍数である前提をコメントに書くか、コードで保証する。

## 完了条件

- `pa_stream_write` がフレーム境界に揃った長さで呼ばれる。
- バッファ末尾に古いデータが残らない。
- コメントが実装と一致している。
- `cargo test --workspace` が通る。
