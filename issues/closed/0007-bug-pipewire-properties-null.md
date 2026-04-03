# PipeWire の properties 生成失敗時に NULL をそのまま使っている

Created: 2026-04-03
Model: Opus 4.6

## 概要

`audio_pipewire.c` の `session_start` で `pw_properties_new` の戻り値を検証せず、NULL のまま `pw_properties_set` に渡していた。

## 根拠

`pw_properties_new` はメモリ確保に失敗すると NULL を返す。`pw_properties_set` に NULL を渡すと未定義動作となり、クラッシュする。

## 再現手順

1. メモリ逼迫状態で `session_start` を呼ぶ
2. `pw_properties_new` が NULL を返す
3. `pw_properties_set(NULL, ...)` で未定義動作

## 解決方法

`pw_properties_new` の戻り値が NULL の場合、既存の `pw_stream_new` 失敗時と同じクリーンアップ処理を行い `-4` を返すようにした。

Completed: 2026-04-03
