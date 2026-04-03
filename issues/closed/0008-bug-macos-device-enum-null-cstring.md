# macOS のデバイス列挙で NULL / 不正 C 文字列を生成し得る

Created: 2026-04-03
Completed: 2026-04-03
Model: Opus 4.6

## カテゴリ

bug

## 概要

`audio_c.m` の `add_device_to_array()` で `malloc()` と `CFStringGetCString()` の戻り値を検証していない。

## 根拠

`malloc()` が NULL を返した場合、直後の `CFStringGetCString()` に NULL バッファを渡してクラッシュする。
また `CFStringGetCString()` が失敗した場合、未初期化バッファが NUL 終端されないまま `device->name` / `device->unique_id` に格納され、Rust 側の `CStr::from_ptr()` が範囲外読みを起こす。

## 再現条件

- メモリ逼迫時に `malloc()` が NULL を返す
- CoreAudio が不正な CFString を返す（極めて稀だが理論上可能）

## 解決方法

`malloc()` の戻り値が NULL の場合、および `CFStringGetCString()` が false を返した場合に、確保済みメモリを解放して `-1` を返すようにした。
