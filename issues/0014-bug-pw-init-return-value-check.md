# pw_init の戻り値チェックを追加する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

bug

## 概要

`audio_pipewire.c` において、`pw_init(NULL, NULL)` の戻り値をチェックしていない。初期化失敗状態で後続の PipeWire API を呼び出すと未定義動作となる。

## 対象箇所

- `src/audio_pipewire.c:163` — デバイス列挙時の `pw_init(NULL, NULL);`
- `src/audio_pipewire.c:373` — セッション作成時の `pw_init(NULL, NULL);`

## 根拠

`pw_init()` は `int` を返し、失敗時は負の errno 値となる（PipeWire API ドキュメント参照）。戻り値を無視した場合、ロックやメモリなどの内部状態が未初期化のまま後続の `pw_main_loop_new()` や `pw_thread_loop_new()` が呼ばれ、クラッシュまたは未定義動作となる。

## 再現条件

- システムで PipeWire デーモンが利用不可能な場合
- メモリ不足時
- ロックファイルの競合時

## 対応方針

`pw_init()` の戻り値をチェックし、負の値が返った場合はエラーを返す。戻り値 0 (成功) または正の値 (既に初期化済み) のみ処理を継続する。

また、`pw_init()` は複数回呼び出しても安全であることを確認し（PipeWire 0.3 では参照カウント方式）、列挙時とセッション作成時の両方で呼ばれている現在のパターンを維持するか、静的フラグで 1 回呼び出しに制限するか判断する。
