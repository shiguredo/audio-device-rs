# 再生コールバックが返す PlaybackFrame のメタデータを検証する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-playback-frame-metadata-handling
- Polished: {YYYY-MM-DD}

## 目的

再生コールバックが返す `PlaybackFrame` の `channels` / `sample_rate` / `frames` が実装で無視されている状態を解消し、利用者が要求と異なる形式を返した場合にサイレントに壊れた音声を再生しないようにする。

## 現状

- `playback_ffi.rs` の `playback_callback` は `write_playback_frame_to_buffer(&frame.data, frame.format, dst, audio_format, channels as usize)` と呼び、変換の前提にバックエンドが要求した `channels` を使う。`frame.channels` / `frame.sample_rate` / `frame.frames` は参照していない。
- `playback_wasapi.rs` の `playback_thread_func` も同じく、要求された `channels` を `dst_channels` として使う。
- `PlaybackFrame` のフィールドはすべて public であり、不整合な値も構築できる。モノラルで生成したフレームを 2 ch デバイスへ返すと L/R に別サンプルが並ぶ誤った音声になるが、エラーにも無音にもならない。
- `AudioPlayback::new` の doc には戻り値の形式を要求値と一致させなければならないという契約が無い。

## 設計方針

- 再生経路で `frame.channels != channels` または `frame.sample_rate != sample_rate` を検出した場合は無音として扱う (または明示的なチャンネル複製・リサンプルを行う)。
- `AudioPlayback::new` と `PlaybackFrame` の rustdoc に、戻り値の `channels` / `sample_rate` は要求値と一致させることを明記する。
- 可能なら `PlaybackFrame` のフィールドを非公開にして不変条件を強制する (後方互換のない変更になるため、公開 API の扱いは別途判断する)。
- 不整合フレームを返した場合の回帰テストを追加する。

## 完了条件

- 要求と異なる `channels` / `sample_rate` を返しても、壊れた音声が再生されない。
- 契約が rustdoc に書かれている。
- 不整合フレームのテストが追加されている。
- `cargo fmt` / `cargo clippy` / `cargo test` が通る。
