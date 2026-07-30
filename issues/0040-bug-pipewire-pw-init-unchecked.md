# audio_pipewire.c pw_init の戻り値未チェックと CHANGES.md の記載不整合を修正する

- Created: 2026-07-30
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pipewire-pw-init-check
- Polished: {YYYY-MM-DD}

## 目的

`audio_pipewire.c` の `pw_init` 呼び出し 3 箇所で戻り値がチェックされていない問題を修正し、CHANGES.md の記載とコードの実態を整合させる。

## 現状

CHANGES.md の 2026.2.0 セクションに「[FIX] pw_init の戻り値未チェックを修正する」と記載されているが、コードでは修正が存在しない。

`src/audio_pipewire.c` 内の 3 箇所すべてで `pw_init(NULL, NULL)` の戻り値を無視している:

1. `audio_pipewire_enumerate_devices` 関数内
2. `audio_pipewire_session_create` 関数内
3. `audio_pipewire_playback_session_create` 関数内

`pw_init` は PipeWire ライブラリの初期化を行い、失敗時は負の値を返す。初期化に失敗した状態で PipeWire API を呼び出すと未定義動作やクラッシュの可能性がある。

## 設計方針

`pw_init` の戻り値をチェックし、失敗時はエラーを返す。`pw_init` は複数回呼んでも安全（冪等）であるため、3 箇所すべてでチェックする。

## 完了条件

`pw_init` の戻り値が 3 箇所すべてでチェックされ、失敗時にエラーが返されること。CHANGES.md の記載とコードの実態が整合すること。

## 解決方法

`src/audio_pipewire.c` の 3 箇所の `pw_init(NULL, NULL)` 呼び出しに戻り値チェックを追加する:

1. `audio_pipewire_enumerate_devices`: 失敗時に `-2` 相当のエラーを返す
2. `audio_pipewire_session_create`: 失敗時に `NULL` を返す
3. `audio_pipewire_playback_session_create`: 失敗時に `NULL` を返す

CHANGES.md の該当エントリは、修正完了後に `## develop` セクションに移動するか、2026.2.0 の記載が正しいことをコードで裏付ける。
