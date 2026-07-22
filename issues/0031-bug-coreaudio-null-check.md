# audio_coreaudio.m の NULL チェック追加

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-coreaudio-null-check
- Polished: 2026-07-21

## 目的

`src/audio_coreaudio.m` に存在する 2 つの NULL/nil 防御チェック欠如を修正する。

1. `audio_input_callback` 内の `start_time` NULL チェック欠如
2. `find_device_by_uid` の `targetUID` nil チェック欠如

## 優先度根拠

API 契約上 NULL/nil が許容されていないが、防御的にチェックする。実運用上は AudioQueue が常に有効ポインタを渡し、Rust 側の UTF-8 保証により不正入力は到達不可能だが、C/ObjC 境界の防御的コーディングとして追加する。

## 現状

### start_time NULL チェック（src/audio_coreaudio.m:40-49）

```c
if (start_time->mFlags & kAudioTimeStampHostTimeValid) {
```

`start_time` が NULL の場合ここでセグメンテーションフォルトになる。AudioQueue のコールバック仕様上 `inStartTime` は常に有効ポインタであり、実運用上 NULL になる条件は存在しないが、防御的にチェックする。

### find_device_by_uid nil チェック（src/audio_coreaudio.m:413, 425）

```c
NSString* targetUID = [NSString stringWithUTF8String:uid]; // 413 行目
// ...
if ([(__bridge NSString*)deviceUID isEqualToString:targetUID]) { // 425 行目
```

`uid` が不正 UTF-8 の場合 `stringWithUTF8String:` は nil を返す。`isEqualToString:` に nil を渡すと API 契約上未定義であり、将来の挙動が保証されない。現コードベースでは Rust の `String`（UTF-8 保証）→ `CString::new()`（null byte チェック済み）→ `const char*` の経路のため、不正 UTF-8 は到達不可能。

## 設計方針

防御的チェックを追加する。`start_time` が NULL の場合は `timestamp_us = 0` としてコールバックを継続する（コールバック自体はスキップしない。音声データは有効なため）。`targetUID` が nil の場合は `kAudioObjectUnknown` を返す（呼び出し元は既に `kAudioObjectUnknown` をエラーとして処理しているため後方互換の問題なし）。

## 完了条件

- `start_time` の NULL チェックが追加される
- `targetUID` が nil の場合に早期リターンする
- `cargo clippy` / `cargo test` が通る（C/ObjC のコールバック内 NULL チェックは Rust のテストで直接検証できないため、既存テストでの回帰確認とコードレビューで担保する）

## 解決方法

1. `audio_coreaudio.m:40` を `if (start_time && (start_time->mFlags & kAudioTimeStampHostTimeValid))` に変更する。NULL 時は `timestamp_us = 0` のままコールバックを継続する
2. `audio_coreaudio.m:413` の直後に `if (!targetUID) { free(deviceIDs); return kAudioObjectUnknown; }` を追加する

## 後方互換

公開 API の変更なし。防御的チェックの追加であり、正常パスの挙動は不変
