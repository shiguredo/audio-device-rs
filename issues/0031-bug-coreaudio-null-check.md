# audio_coreaudio.m の NULL チェック追加

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-coreaudio-null-check
- Polished: {YYYY-MM-DD}

## 目的

`src/audio_coreaudio.m` に存在する 2 つの NULL 参照リスクを修正する。

1. `audio_input_callback` 内の `start_time` NULL チェック欠如
2. `find_device_by_uid` の不正 UTF-8 入力時の nil 参照

## 優先度根拠

`start_time` が NULL の場合セグメンテーションフォルトでクラッシュする。`find_device_by_uid` の nil 参照は動作未定義。

## 現状

### start_time NULL チェック（src/audio_coreaudio.m:40-49）

```c
if (start_time->mFlags & kAudioTimeStampHostTimeValid) {
    // start_time が NULL の場合ここでクラッシュ
```

### find_device_by_uid nil 参照（src/audio_coreaudio.m:355-370）

```c
NSString* targetUID = [NSString stringWithUTF8String:uid];
// uid が不正 UTF-8 の場合 targetUID は nil
if ([(__bridge NSString*)deviceUID isEqualToString:targetUID]) {
    // nil 引数での呼び出しは動作未定義
```

## 完了条件

- `start_time` の NULL チェックが追加される
- `targetUID` が nil の場合に早期リターンする
- `cargo clippy` / `cargo test` が通る

## 解決方法

1. `if (start_time && (start_time->mFlags & kAudioTimeStampHostTimeValid))` と NULL チェックを追加する
2. `targetUID` が nil の場合に `free(deviceIDs); return kAudioObjectUnknown;` で早期リターンする
