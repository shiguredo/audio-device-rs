# PipeWire の process callback で chunk と n_datas を未検証で参照している

Created: 2026-04-03
Model: Opus 4.6

## 概要

`audio_pipewire.c` の `on_process` コールバックで `spa_buf->datas[0].chunk` が NULL のケースや `n_datas` が 0 のケースを考慮せずに参照していた。

## 根拠

`datas[0].data` の NULL チェックは既に行っているが、`n_datas` の範囲チェックと `chunk` の NULL チェックが欠落しており、防御的チェックとして一貫性がない。PipeWire 側から不正なバッファが渡された場合に NULL 参照でクラッシュする可能性がある。

## 再現手順

1. PipeWire から `n_datas == 0` または `chunk == NULL` のバッファが渡される状況を作る
2. `on_process` コールバックが呼ばれる
3. NULL 参照でクラッシュする

## 解決方法

`spa_buf->n_datas < 1` と `!spa_buf->datas[0].chunk` のチェックを既存の `!spa_buf->datas[0].data` チェックと統合した。条件に該当する場合はバッファを戻して処理を打ち切る。

Completed: 2026-04-03
