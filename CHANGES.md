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

- [ADD] cargo-fuzz を用いた fuzzing ターゲットを追加する
  - @melpon
- [FIX] C コード内の strdup 戻り値 NULL チェック欠落を修正する
  - @melpon
- [FIX] from_ffi() が未知の FFI 定数値を無条件に Input/S16 へ丸め込む問題を修正する
  - @melpon
- [FIX] PulseAudio デバイス列挙でコンテキスト切断時に無限ループする問題を修正する
  - @melpon
- [FIX] PlaybackFrame::from_s16/from_f32 での usize から i32 へのキャストによる整数オーバーフローを防止する
  - @melpon
- [FIX] pw_init の戻り値未チェックを修正する
  - @melpon

### misc

- [FIX] SendHandle / SendPtr の重複定義を共通化する
  - @melpon
- [FIX] determine_audio_format の重複定義を統合する
  - @melpon
- [FIX] examples のエラーメッセージを日本語から英語に修正する
  - @melpon
- [FIX] PBT から「パニックしないこと」のみを検証するテストを削除する
  - @melpon
- [UPDATE] プラットフォーム間で重複していた共通型を src/common.rs に抽出する
  - @melpon
- [ADD] Error 型の PBT テストを追加する
  - @melpon

## 2026.1.0

**リリース日**: 2026-04-03
