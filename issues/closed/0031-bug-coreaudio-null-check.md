# audio_coreaudio.m の NULL チェック追加

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-10-27
- Model: Qwen 3
- Branch: feature/fix-coreaudio-null-check
- Polished: 2026-07-21

## 目的

`src/audio_coreaudio.m` の `find_device_by_uid` に存在する `targetUID` の nil チェック欠如を修正する。

## 優先度根拠

`[NSString stringWithUTF8String:]` は不正な UTF-8 に対して nil を返す。`find_device_by_uid` は `const char*` を入力として受け取るため、入力が UTF-8 として妥当であることを型では保証できない。Rust 側の `String` と `CString::new()` を経由する現状の呼び出し経路では不正 UTF-8 は到達不可能だが、C/ObjC 境界の防御的コーディングとしてチェックを追加する。

## 現状

### find_device_by_uid の targetUID nil チェック

`src/audio_coreaudio.m` の `find_device_by_uid` では、次のように `uid` から `NSString` を生成して `isEqualToString:` に渡している。

```c
NSString* targetUID = [NSString stringWithUTF8String:uid];
// ...
if ([(__bridge NSString*)deviceUID isEqualToString:targetUID]) {
```

`uid` が不正な UTF-8 の場合 `stringWithUTF8String:` は nil を返す。`isEqualToString:` に nil を渡すと API 契約上未定義であり、将来の挙動が保証されない。

`audio_input_callback` の `start_time` は AudioQueue のコールバック契約上 NULL にならないため、チェック対象外とする。

## 設計方針

`targetUID` が nil の場合は `kAudioObjectUnknown` を返す。呼び出し元は既に `kAudioObjectUnknown` をエラーとして処理しているため、後方互換の問題はない。

## 完了条件

- `targetUID` が nil の場合に `kAudioObjectUnknown` を返す
- `cargo clippy` / `cargo test` が通る（C/ObjC の nil チェックは Rust のテストで直接検証できないため、既存テストでの回帰確認とコードレビューで担保する）

## 解決方法

`find_device_by_uid` の `NSString* targetUID = [NSString stringWithUTF8String:uid];` の直後に、`deviceIDs` を解放して `kAudioObjectUnknown` を返す nil チェックを追加する。

## 後方互換

公開 API の変更なし。防御的チェックの追加であり、正常パスの挙動は不変
