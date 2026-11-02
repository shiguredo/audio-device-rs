# PipeWire の列挙ハングと境界データ処理を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pipewire-boundary-handling
- Polished: {YYYY-MM-DD}

## 目的

PipeWire のデバイス列挙がサーバ切断時に永久に戻らない問題と、キャプチャ・再生のバッファ境界処理の不備を修正する。

## 現状

- `audio_pipewire_enumerate_devices` は `pw_main_loop_run` を `enum_core_done` だけで終了させており、core の error listener もタイムアウトも無い。サーバが `done` を返す前に切断すると永久に戻らない。`enum_core_events` は `.done` のみを登録している。
- `on_process` (キャプチャ) は `spa_buf->datas[0].data` から直接読み、`chunk->size` を `maxsize` にクランプするだけで、`chunk->offset` と `chunk->stride` を参照していない。PipeWire の `spa_chunk.offset` は「有効データの先頭」であり、`data + (offset % maxsize)` から読む必要がある。`SPA_CHUNK_FLAG_CORRUPTED` などの flags も確認していない。
- `spa_format_audio_raw_build` の戻り値 (失敗時 NULL) を検査せず、`params[0]` に入れて `pw_stream_connect` へ渡している。
- `pw_buffer.requested` のバージョン判定が `PW_CHECK_VERSION(0, 3, 49)` になっているが、`requested` フィールドは 0.3.50 で追加された。0.3.49 のヘッダではコンパイルできない。
- セッションの非同期障害 (`core_error`) は開始待ちの `wait_for_stream_ready` の中でしか参照されず、開始成功後の障害は利用者へ伝わらない。

## 設計方針

- デバイス列挙に core の error listener とタイムアウト (ウォッチドッグ) を追加し、サーバ切断時にエラーで戻るようにする。
- キャプチャの process コールバックで `chunk->offset` を `maxsize` で剰余して先頭を決め、`size` を `maxsize` にクランプする。`stride` が想定と異なる場合はスキップするか、フレームごとの読み出しに対応する。`SPA_CHUNK_FLAG_CORRUPTED` はスキップする。
- `spa_format_audio_raw_build` の NULL を検査して失敗させる。
- `PW_CHECK_VERSION(0, 3, 49)` を 0.3.50 に修正するか、build.rs で最低バージョンを検査する。
- 開始後の非同期障害を記録し、stop または状態取得 API から返す (または doc で「通知されない」と明記する)。

## 完了条件

- PipeWire サーバが列挙中に切断しても `AudioDeviceList::enumerate_pipewire()` が有限時間でエラーを返す。
- `chunk->offset` を考慮したデータ読み出しになっている。
- NULL の format pod を `pw_stream_connect` に渡さない。
- PipeWire 0.3.49 でビルドが壊れない (または最低バージョンが明示される)。
- テストまたは再現手順が追加され、`cargo test --workspace` が通る。
