# pw_init の戻り値チェックを追加する

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## 概要

`audio_pipewire.c` において、`pw_init(NULL, NULL)` の戻り値をチェックしていない。初期化失敗状態で後続の PipeWire API を呼び出すと未定義動作となる。

## 対象箇所

- `src/audio_pipewire.c:163` — デバイス列挙時の `pw_init(NULL, NULL);`
- `src/audio_pipewire.c:373` — セッション作成時の `pw_init(NULL, NULL);`

## 根拠

`pw_init()` は `int` を返し、失敗時は負の errno 値となる（PipeWire API ドキュメント参照）。戻り値を無視した場合、ロックやメモリなどの内部状態が未初期化のまま後続の `pw_main_loop_new()` や `pw_thread_loop_new()` が呼ばれ、クラッシュまたは未定義動作となる。

## 再現条件

- システムで PipeWire デーモンが利用不可能な場合
- メモリ不足時
- ロックファイルの競合時

## 対応方針

`pw_init()` の戻り値をチェックし、負の値が返った場合はエラーを返す。戻り値 0（成功）または正の値（既に初期化済み、PipeWire 0.3 では参照カウント方式）のみ処理を継続する:

```c
// 変更後
int err = pw_init(NULL, NULL);
if (err < 0) {
    return -1;  // または適切なエラーハンドリング
}
```

また `pw_init()` は複数回呼び出しても安全（PipeWire 0.3 では参照カウント方式）。現状の「列挙時とセッション作成時の両方で呼ぶ」パターンを維持する。静的フラグによる 1 回制限は PipeWire の内部参照カウントと競合する可能性があるため導入しない。

## CHANGES.md への追記

`## develop` セクションに以下のエントリを追記する:

```
- [FIX] pw_init の戻り値未チェックを修正する
  - @ユーザー名
```

## テスト戦略

- PipeWire デーモンが利用不可能な環境でのテストは CI で再現が困難なため、コードレビューによる検証を主とする
- 通常の PipeWire 環境でデバイス列挙とセッション作成が引き続き正常に動作することを手動で確認する
