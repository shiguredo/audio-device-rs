# strdup の戻り値 NULL チェックを追加する

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## 概要

`audio_pulse.c` と `audio_pipewire.c` のデバイス列挙コールバック内で、`strdup()` の戻り値をチェックしていない。メモリ不足時には `NULL` が返り、`device->name` / `device->unique_id` が NULL のまま列挙リストに追加される。結果として Rust 側の `AudioDevice::name()` / `unique_id()` が `Err(Error::NullPointer(...))` を返し、当該デバイスが正常に扱えなくなる。

注意: Rust 側の `device.rs:52-53, 60-61` には `is_null()` チェックが存在するため、即座のクラッシュは発生しない。しかし不完全なデバイスが列挙リストに混入し、ユーザーが不正なデバイスを選択する可能性がある。

## 対象箇所

- `src/audio_pulse.c:86` — source_info_callback 内の `strdup(name)`
- `src/audio_pulse.c:88` — 同、`strdup(unique_id)`
- `src/audio_pulse.c:136` — sink_info_callback 内の `strdup(name)`
- `src/audio_pulse.c:137` — 同、`strdup(unique_id)`
- `src/audio_pipewire.c:110` — enum_registry_global 内の `strdup(name)`
- `src/audio_pipewire.c:111` — 同、`strdup(unique_id)`

## 非対象

- `audio_pulse.c:388`, `audio_pipewire.c:385` の `session->device_id = device_id ? strdup(device_id) : NULL;` は未チェックだが、`device_id == NULL` は「デフォルトデバイスを使用する」という有効な状態であり、strdup 失敗時も後続の `free(session->device_id)` が NULL を受け取っても安全（`free(NULL)` は POSIX で定義済み）。strdup 失敗時は意図したデバイス指定が無視されるが、データ破損やクラッシュには至らないため本 issue のスコープ外とする。

## 根拠

POSIX `strdup(3)` はメモリ不足時 `NULL` を返す。同コールバック内では `calloc` や `realloc` の戻り値はチェックされているが `strdup` だけチェックされていない。戻り値が NULL のまま `device->name` や `device->unique_id` に設定されると、後続の Rust 側で `NullPointer` エラーが発生し、不完全なデバイスが列挙リストに混入する。

## 再現条件

- システムのメモリが逼迫している状態でデバイス列挙を実行した場合
- `strdup` 内部の `malloc` が `NULL` を返した場合

## 対応方針

各コールバック内の `calloc` 成功後に、以下のパターンで `strdup` の戻り値をチェックする:

```c
struct AudioDevice* device = calloc(1, sizeof(struct AudioDevice));
if (!device) {
    return;
}

device->name = strdup(some_name);
if (!device->name) {
    free(device);
    return;
}

device->unique_id = strdup(some_id);
if (!device->unique_id) {
    free(device->name);
    free(device);
    return;
}
```

このパターンを 3 箇所（`source_info_callback`, `sink_info_callback`, `enum_registry_global`）すべてに適用する。

## CHANGES.md への追記

`## develop` セクションに以下のエントリを追記する:

```
- [FIX] C コード内の strdup 戻り値 NULL チェック欠落を修正する
  - @ユーザー名
```

## テスト戦略

- C コードの `strdup` 失敗パスをテストするには `malloc` の挙動を外部から制御する必要がある。本プロジェクトでは C コードの単体テストフレームワークを持たないため、コードレビューによる目視検証を主とする
- 代替として、Rust 側の PBT (`pbt/tests/prop_device.rs` を新設) で NullPointer エラーを適切に扱えることを検証する
- 本修正が適用された後に、通常のデバイス列挙が正常に動作することを手動で確認する
