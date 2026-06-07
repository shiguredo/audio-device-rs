# 変更履歴

- UPDATE
  - 後方互換がある変更
- ADD
  - 後方互換がある追加
- CHANGE
  - 後方互換のない変更
- FIX
  - バグ修正

## develop

- [FIX] from_ffi() が未知の FFI 定数値を無条件に Input/S16 へ丸め込む問題を修正する
  - @melpon

### misc

- [UPDATE] プラットフォーム間で重複していた共通型を src/common.rs に抽出する
  - @melpon


## 2026.1.0

**リリース日**: 2026-04-03
