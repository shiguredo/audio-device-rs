# PipeWire の列挙ハング・バッファ境界処理・非同期障害通知の不備を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pipewire-boundary-handling
- Polished: 2026-11-03

## 目的

PipeWire バックエンドの次の不備を修正する。

- デバイス列挙がサーバ切断時に永久に戻らない
- キャプチャ・再生のバッファ境界処理が `spa_chunk` の契約に従っていない
- `spa_format_audio_raw_build` の失敗が検査されていない
- `pw_buffer.requested` のバージョン判定が誤っている
- セッション開始後の非同期障害が利用者へ伝わらない

## 現状

- `audio_pipewire_enumerate_devices` は `pw_main_loop_run` を `enum_core_done` だけで終了させており、core の error listener もタイムアウトも無い。サーバが `done` を返す前に切断すると永久に戻らない。`enum_core_events` は `.done` のみを登録している。
- `on_process` (キャプチャ) は `spa_buf->datas[0].data` から直接読み、`chunk->size` を `maxsize` にクランプするだけで、`chunk->offset` と `chunk->stride` を参照していない。PipeWire の `spa_chunk.offset` は「有効データの先頭」であり、`data + (offset % maxsize)` から読む必要がある。`offset` を考慮しないため、`offset` が 0 でないバッファでは先頭の無効データを読む。`SPA_CHUNK_FLAG_CORRUPTED` などの flags も確認していない。
- `spa_format_audio_raw_build` の戻り値 (失敗時 NULL) を検査せず、`params[0]` に入れて `pw_stream_connect` へ渡している。
- `pw_buffer.requested` のバージョン判定が `PW_CHECK_VERSION(0, 3, 49)` になっているが、`requested` フィールドは 0.3.50 で追加された。0.3.49 のヘッダではコンパイルできない。
- セッションの非同期障害 (`core_error`) は開始待ちの `wait_for_stream_ready` の中でしか参照されず、開始成功後の障害は利用者へ伝わらない。Rust 側の `FfiCaptureImpl::stop` / `FfiPlaybackImpl::stop` は C の `session_stop` の戻り値を捨てているため、C 側で `stop` からエラーを返しても公開 API には届かない。

## 設計方針

- デバイス列挙に core の error listener とタイムアウト (ウォッチドッグ) を追加し、サーバ切断時にエラーで戻るようにする。タイムアウトは既存の `STREAM_STATE_TIMEOUT_SEC` と同じ 10 秒を上限とする。タイムアウトまたは core エラーで戻る場合は、列挙済みの `ctx.devices` を `audio_pipewire_free_devices` で解放してから負の値を返す。
- キャプチャの process コールバックで `chunk->offset` を `maxsize` で剰余して先頭を決め、`size` を `maxsize - offset` にクランプする。`maxsize` が 0 の場合はそのバッファをスキップする。
- `stride` が 0 以外で `frame_size` (`2 * channels`) と異なる場合は、そのバッファをスキップする。`stride` が 0 の場合は `frame_size` として扱う。
- `SPA_CHUNK_FLAG_CORRUPTED` が立つバッファはスキップする。`SPA_CHUNK_FLAG_EMPTY` は無音を表すため、そのまま渡す。
- `spa_format_audio_raw_build` の NULL を検査して失敗させる。戻り値は既存の `AudioPipeWireResult` の負の値を使う。
- `PW_CHECK_VERSION(0, 3, 49)` を `PW_CHECK_VERSION(0, 3, 50)` に修正する。0.3.49 では `requested` を使わず、`max_frames` にフォールバックする。
- 開始後の非同期障害を記録し、stop または状態取得 API から返す。stop から返す場合は、Rust 側の `FfiCaptureImpl::stop` / `FfiPlaybackImpl::stop` と `AudioCaptureSession::stop` / `AudioPlaybackSession::stop` も戻り値を受け取れるように変更する。API を増やさない場合は rustdoc に「開始後の非同期障害は通知されない」と明記する。
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加する。公開 API のシグネチャを変更する場合は `[CHANGE]` も追加する。

## 完了条件

- PipeWire サーバが列挙中に切断しても `AudioDeviceList::enumerate_pipewire()` が有限時間でエラーを返す。エラー時は列挙済みのデバイスが解放される。
- `chunk->offset` を考慮し、`offset + size` が `maxsize` を超えないデータ読み出しになっている。`maxsize` が 0 の場合はスキップする。
- `stride` が 0 以外で `frame_size` と異なるバッファはスキップされる。
- `SPA_CHUNK_FLAG_CORRUPTED` のバッファはスキップされる。
- NULL の format pod を `pw_stream_connect` に渡さない。
- PipeWire 0.3.49 でビルドが壊れない。
- 開始後の非同期障害を stop または状態取得 API から取得できる、または通知されないことが rustdoc に明記されている。
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追加されている。公開 API のシグネチャを変更する場合は `[CHANGE]` も追加されている。
- テストまたは再現手順が追加され、CI の PipeWire ジョブと同じ `cargo test --workspace --no-default-features --features default-pipewire` が通る。
