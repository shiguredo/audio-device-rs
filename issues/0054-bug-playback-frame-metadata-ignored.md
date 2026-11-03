# 再生コールバックが返す PlaybackFrame の channels / sample_rate を検証する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-playback-frame-metadata-handling
- Polished: 2026-11-03

## 目的

再生コールバックが返す `PlaybackFrame` の `channels` / `sample_rate` が実装で無視されている状態を解消し、利用者がコールバック引数と異なる形式を返した場合にサイレントに壊れた音声を再生しないようにする。`frames` は `data` の長さから書き込みフレーム数を決めるため再生経路では参照しないが、`from_s16` / `from_f32` が `frames` と `data` の整合を保証しており、要求フレーム数との差は無音埋めまたは切り詰めで処理される。

## 現状

- `src/playback_ffi.rs` の `playback_callback` は `write_playback_frame_to_buffer(&frame.data, frame.format, dst, audio_format, channels as usize)` と呼び、変換の前提にバックエンドが要求した `channels` を使う。`frame.channels` / `frame.sample_rate` / `frame.frames` は参照していない。
- `src/playback_wasapi.rs` の `playback_thread_func` も同じく、要求された `channels` を `dst_channels` として使う。
- `PlaybackFrame` のフィールドはすべて public であり、不整合な値も構築できる。モノラルで生成したフレームを 2 ch デバイスへ返すと L/R に別サンプルが並ぶ誤った音声になるが、エラーにも無音にもならない。
- `frame.format` は変換元の形式として使われ、バックエンドの形式と異なっても S16 / F32 の変換が行われる。`frame.frames` は書き込みフレーム数の決定に使わず、書き込みフレーム数は `frame.data` の長さから決まる。`from_s16` / `from_f32` は `frames` と `data` の整合を保証しており、`data` が要求フレーム数より短い場合は不足分が無音で埋められ、長い場合はバッファ長に合わせて切り詰められる。
- `AudioPlayback::new` / `new_coreaudio` / `new_pulse` / `new_pipewire` / `new_wasapi` の doc には、コールバックが返す `PlaybackFrame` の `channels` / `sample_rate` をコールバック引数と一致させなければならないという契約が無い。`PlaybackFrame` の doc にも、`data` のインターリーブ形式、`frames` と `data` の整合条件、および `channels` / `sample_rate` の契約が書かれていない。

## 設計方針

- 再生経路で `frame.channels != channels` または `frame.sample_rate != sample_rate` を検出した場合は、その呼び出しを無音として扱う。本 issue ではチャンネル複製・リサンプルは行わない。
  - FFI 経路では `playback_callback` が 0 を返す。`src/audio.h` の `AudioPlaybackCallback` の契約では 0 は無音であり、C 側がバッファを無音で埋める。
  - WASAPI 経路では、ユーザーコールバックが `None` を返した場合と同じく、`AUDCLNT_BUFFERFLAGS_SILENT` を指定して `ReleaseBuffer` する。
- 検証ロジックは `src/common.rs` の共通関数に置き、`src/playback_ffi.rs` と `src/playback_wasapi.rs` の両方から呼ぶ。`src/common.rs` の `#[cfg(test)]` に、`channels` / `sample_rate` が要求値と一致しない `PlaybackFrame` を検出する単体テストを追加する。`src/playback_ffi.rs` の `#[cfg(test)]` には、不整合フレームを返すコールバックで `playback_callback` が 0 を返すテストを追加する。
- `AudioPlayback::new` / `new_coreaudio` / `new_pulse` / `new_pipewire` / `new_wasapi` と `PlaybackFrame` の rustdoc に、コールバックが返す `PlaybackFrame` の `channels` / `sample_rate` はコールバック引数の `channels` / `sample_rate` と一致させること、一致しない場合は無音として扱われることを明記する。`PlaybackFrame` の doc には、`data` が `format` の形式で `channels` 個のサンプルをインターリーブしたものであること、`frames` が `data` に含まれるサンプルフレーム数であることも書く。
- 本 issue では `PlaybackFrame` の公開 API を変更しない。`frame.channels` / `frame.sample_rate` が `frame.data` の正しい記述であることを前提に、要求値との不一致だけを検出する。

## 完了条件

- 要求と異なる `channels` / `sample_rate` を返した場合、そのコールバック呼び出しは無音として扱われ、壊れた音声が再生されない。
- `AudioPlayback::new` / `new_coreaudio` / `new_pulse` / `new_pipewire` / `new_wasapi` と `PlaybackFrame` の rustdoc に、コールバック引数との一致契約と不一致時の無音扱い、および `PlaybackFrame` の `data` / `frames` の意味が書かれている。
- `channels` / `sample_rate` が要求値と一致しない `PlaybackFrame` を検出する単体テストが `src/common.rs` に追加され、`playback_callback` が 0 を返すことが `src/playback_ffi.rs` のテストで確認されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が、CI の全構成 (PulseAudio / PipeWire / CoreAudio / WASAPI) で通る。
