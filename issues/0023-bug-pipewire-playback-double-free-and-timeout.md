# audio_pipewire.c の pw_properties 二重解放とタイムアウト追加

- Priority: High
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-pipewire-playback-double-free-and-timeout
- Polished: 2026-07-21

## 目的

`src/audio_pipewire.c` に存在する 2 つのバグを修正する。

1. 再生セッション開始失敗時の `pw_properties` 二重解放
2. ストリーム状態待ちループにタイムアウトがなくハングする可能性

## 優先度根拠

二重解放はヒープ破壊を引き起こす経路。タイムアウトなしは PipeWire デーモンが無応答に陥った場合に呼び出しスレッドが永久にブロックする。

## 現状

### 二重解放（src/audio_pipewire.c:856-859）

```c
// 856-857 行目
session->stream =
    pw_stream_new(session->core, "audio-playback", props);
// 858-859 行目
if (!session->stream) {
    pw_properties_free(props); // 859 行目: 二重解放
```

`pw_stream_new` は成否に関わらず `props` の所有権を取得する（PipeWire API ドキュメント: `props` — stream properties, **ownership is taken**）。したがって `pw_stream_new` が失敗して `NULL` を返した場合でも `props` は既に解放済みであり、859 行目の `pw_properties_free(props)` は二重解放になる。

キャプチャ側（`audio_pipewire_session_start`、500-508 行目）では同じ失敗パスで `pw_properties_free` を呼んでおらず、こちらが正しい。

### タイムアウトなし（src/audio_pipewire.c:546-568, 904-929）

キャプチャ側（546-568 行目）と再生側（904-929 行目）の両方で、ストリーム状態待ちループが `pw_thread_loop_wait`（無限待機）を使用している。

```c
// 546 行目（キャプチャ側）/ 904 行目（再生側）
while (1) {
    enum pw_stream_state state = pw_stream_get_state(session->stream, NULL);
    if (state == PW_STREAM_STATE_STREAMING) break;
    if (state == PW_STREAM_STATE_PAUSED) {
        pw_stream_set_active(session->stream, true);
        break;
    }
    if (state == PW_STREAM_STATE_ERROR ||
        state == PW_STREAM_STATE_UNCONNECTED) {
        // ... エラー処理 ...
        return -5;
    }
    pw_thread_loop_wait(session->thread_loop); // 567 行目 / 928 行目: 無限待機
}
```

`PW_STREAM_STATE_CONNECTING` 等の中間状態でシグナルが来ない場合、ループが永久にブロックする。

特に再生側は深刻である。`playback_on_stream_state_changed`（688-697 行目）は STREAMING / ERROR / UNCONNECTED でしかシグナルを送らず、`playback_core_done`（707-711 行目）は何もしない。そのためセッションマネージャー (WirePlumber) が無くストリームが `PAUSED` に留まった場合、ループを起こす手段が一切ない。キャプチャ側は `session_core_done`（352-357 行目）がシグナルを送るため、core done イベントでループが起床し PAUSED を検知できる。

### 再現条件

- 二重解放: `pw_stream_new` が失敗する状況で `audio_pipewire_playback_session_start` を呼ぶと発生する
- タイムアウトなし（CONNECTING）: PipeWire デーモンが無応答に陥り、ストリームが `CONNECTING` 状態のまま遷移しなくなると、呼び出しスレッドが永久にブロックする
- タイムアウトなし（PAUSED、再生側のみ）: WirePlumber が無くストリームが `PAUSED` のままシグナルが来ないと、再生側は永久にブロックする

## 設計方針

- 二重解放: キャプチャ側の正しいパターン（`pw_properties_free` を呼ばない）に揃える
- タイムアウト: `pw_thread_loop_timedwait`（PipeWire 0.3 以降で利用可能）を使用し、ループ全体の経過時刻で判定する。1 回の wait 呼び出しあたりではなく、ループ開始からの累積でタイムアウトを計測する（スプリアスウェイクアップで早期に返っても合計待ち時間が上限を超えないようにするため）
- タイムアウト値: 10 秒（`#define STREAM_STATE_TIMEOUT_SEC 10`）。セッションマネージャー (WirePlumber) によるストリームルーティングの応答時間を考慮した上限値。タイムアウト境界で状態が遷移していても `-5` を返す（最大 1 秒の競合窓があるが、呼び出し側のリトライで回復する）

## 完了条件

- 再生セッション開始失敗時のエラーパスから `pw_properties_free(props)` が削除される
- キャプチャ・再生の両方のストリーム状態待ちループにタイムアウト（10 秒）が設定される
- タイムアウト後は既存の ERROR/UNCONNECTED パスと同じクリーンアップ（`pw_stream_destroy` → `session->stream = NULL` → `spa_hook_remove` → `pw_core_disconnect` → `session->core = NULL` → `pw_thread_loop_unlock` → `pw_thread_loop_stop`）を経てエラーコード `-5` を返す
- `cargo clippy` / `cargo test` が通る（PipeWire は Linux 専用のため、macOS 上のテストではこのコードパスは検証されない。Linux CI でのビルド確認、またはキャプチャ側との差分対照によるコードレビューで担保する）

## 解決方法

1. `audio_pipewire_playback_session_start` の 859 行目 `pw_properties_free(props)` を削除する
2. キャプチャ側（546-568 行目）と再生側（904-929 行目）の `while (1)` ループを以下のように変更する（`time.h` は 7 行目で既にインクルード済み、`clock_gettime` も 311 行目で既に使用されているため新規インクルード不要）:
   - ループ開始前に `struct timespec start_time; clock_gettime(CLOCK_MONOTONIC, &start_time);` で開始時刻を記録する
   - `pw_thread_loop_wait(session->thread_loop)` を `pw_thread_loop_timedwait(session->thread_loop, 1)` に置換する（1 秒ごとに経過時刻を確認する。`timedwait` の戻り値は本設計では不要なため無視する。既存の `pw_thread_loop_wait` も戻り値を無視しており同じ呼び出し規約）
   - `timedwait` の直後に `clock_gettime(CLOCK_MONOTONIC, &now)` を呼び、`now.tv_sec - start_time.tv_sec >= STREAM_STATE_TIMEOUT_SEC` ならタイムアウトとして ERROR/UNCONNECTED パスと同じクリーンアップを経て `return -5` する
3. `#include "audio_pipewire.h"`（12 行目）の直後に空行を挟んで `#define STREAM_STATE_TIMEOUT_SEC 10` を追加する

## 後方互換

`audio_pipewire_session_start` / `audio_pipewire_playback_session_start` の戻り値の仕様が変わる。従来は PipeWire デーモン無応答時に無限ブロックしていたが、修正後は最大 10 秒で `-5` を返す。Rust 側（`playback_ffi.rs` / `capture_ffi.rs`）は `ret < 0` で `Error::SessionStartFailed` を返すため機械的には動作するが、呼び出し側がリトライするか失敗するかの方針に影響しうる
