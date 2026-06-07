# strdup の戻り値 NULL チェックを追加する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

bug

## 概要

`audio_pulse.c` と `audio_pipewire.c` のデバイス列挙コールバック内で、`strdup()` の戻り値をチェックしていない。メモリ不足時には `NULL` が返り、`device->name` / `device->unique_id` が NULL のまま後続処理に渡される。

## 対象箇所

- `src/audio_pulse.c:86` — `device->name = strdup(info->description ? info->description : info->name);`
- `src/audio_pulse.c:88` — `device->unique_id = strdup(info->name);`
- `src/audio_pulse.c:136` — `device->name = strdup(info->description ? info->description : info->name);`
- `src/audio_pulse.c:137` — `device->unique_id = strdup(info->name);`
- `src/audio_pipewire.c:110` — `device->name = strdup(node_description ? node_description : node_name);`
- `src/audio_pipewire.c:111` — `device->unique_id = strdup(node_name);`

## 根拠

POSIX `strdup(3)` はメモリ不足時 `NULL` を返す。戻り値が NULL のまま `device->name` や `device->unique_id` に設定されると、後続の `audio_device_name()` / `audio_device_unique_id()` が NULL を返し、Rust 側の `CStr::from_ptr()` でクラッシュする。

## 再現条件

- システムのメモリが逼迫している状態でデバイス列挙を実行した場合
- `strdup` 内部の `malloc` が `NULL` を返した場合

## 対応方針

各 `strdup()` 呼び出しの戻り値をチェックし、NULL の場合は確保済みの `device` 構造体を解放して処理を継続する（当該デバイスをスキップする）。
