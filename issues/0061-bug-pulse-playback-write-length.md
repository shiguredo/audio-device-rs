# PulseAudio 再生の書き込み長をフレーム境界に揃える

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pulse-playback-write-length
- Polished: 2026-11-03

## 目的

PulseAudio 再生で `pa_stream_write` に渡す長さを常にフレーム境界に揃え、`len` がフレームサイズの倍数でない場合に書き込みが失敗する問題を防ぐ。あわせて、C 側で書き込み済みフレーム数以降をゼロ埋めし、Rust 側の共通変換関数の実装に依存したコメントを C 側の実装だけを説明するコメントに改める。

## 現状

- `src/audio_pulse.c` の `stream_write_callback` は `pa_stream_begin_write` で `len` バイトのバッファを取得し、`frames = len / frame_size` のフレーム数でユーザーコールバックを呼んだあと、`pa_stream_write(s, buf, len, ...)` を呼ぶ。`pa_stream_write` の戻り値は確認していない。
- `pa_stream_begin_write` の戻り値 `len` は、要求した `nbytes` をもとにした memblock の長さであり、フレームサイズの倍数である保証はない。PulseAudio の `pa_stream_write` は `nbytes` がストリームのフレームサイズの倍数であることを要求する。PulseAudio 16.1 の `src/pulse/stream.c` の `pa_stream_write_ext_free` は `length % pa_frame_size(&s->sample_spec) == 0` を検査し、満たさない場合は `-PA_ERR_INVALID` を返す。現行コードは戻り値を無視するため、`len` がフレームサイズの倍数でない場合は書き込みが行われず、`pa_stream_begin_write` が同じ memblock を返す間は同じ `len` で失敗し続ける。
- `src/common.rs` の `write_playback_frame_to_buffer` は、`src/playback_ffi.rs` の `playback_callback` が渡す `frames * channels * bytes_per_sample` のバッファのうち、書き込み済みの領域より後をゼロ埋めする。`stream_write_callback` が `pa_stream_write` に渡す `len` は `frames * frame_size` より大きくなり得るため、`len - frames * frame_size` バイトはこのゼロ埋めの対象外になる。ただし、`len` がフレームサイズの倍数でない場合は書き込み自体が失敗するため、この領域は送られない。
- `stream_write_callback` は `frames <= 0` または `written <= 0` の場合だけ `len` 全体をゼロ埋めし、`0 < written < frames` の場合は Rust 側の共通変換関数が `frames * frame_size` までをゼロ埋めすることを前提にしている。`src/audio.h` の `AudioPlaybackCallback` の契約は「戻り値は実際に書き込んだフレーム数」だけで、書き込み済みフレーム数以降をゼロ埋めすることを要求していない。
- `stream_write_callback` のコメント「written > 0 の場合は Rust 側の共通変換関数がバッファ末尾をゼロ埋めしている」は、C 側のバッファ全体のゼロ埋めを保証しているように読めるが、実際にゼロ埋めされるのは `frames * frame_size` までである。また、C 側のコメントが Rust 側の実装名に依存しており、`CODEBASE.md` の「モジュールの責務を越えた説明を書かないこと」に反する。

## 設計方針

- `src/audio_pulse.c` の `stream_write_callback` で、`pa_stream_write` に渡す長さを常に `frames * frame_size` にする。`len` が `frame_size` の倍数でない場合は、`frames = len / frame_size` で求めた `frames * frame_size` を書き込み長とし、`len` 全体は書き込まない。`pa_stream_write` は `pa_stream_begin_write` が返した領域の一部だけを書き込める。
- `frames <= 0` または `written <= 0` の場合は `len` 全体をゼロ埋めする。`written > 0` の場合は `written` を `frames` にクランプし、`written * frame_size` から `len` までをゼロ埋めする。これにより、C 側は Rust 側の共通変換関数が書き込み済みフレーム数以降をゼロ埋めすることを前提にしない。
- `stream_write_callback` のコメントを「書き込み済みフレーム数まではコールバックが埋め、それ以降はこの関数でゼロ埋めする」に修正する。Rust 側の共通変換関数への言及を削除する。
- `len` が `frame_size` の倍数である前提はコメントに書くだけにせず、上記のとおりコードで保証する。
- `src/audio.h` の `AudioPlaybackCallback` の契約は変更しない。C 側が戻り値のフレーム数以降をゼロ埋めする責務を持つ。

## 完了条件

- `stream_write_callback` が `pa_stream_write` に渡す長さが常に `frames * frame_size` になっている。
- `frames <= 0` または `written <= 0` の場合は `len` 全体がゼロ埋めされ、`written > 0` の場合は `written` を `frames` にクランプしたうえで `written * frame_size` から `len` までがゼロ埋めされている。
- `stream_write_callback` のコメントが C 側の実装だけを説明し、Rust 側の共通変換関数に言及していない。
- `cargo test --workspace` が通る (CI の Ubuntu (pulse) ジョブを含む)。
