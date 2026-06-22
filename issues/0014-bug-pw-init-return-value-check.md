# pw_init の戻り値チェックを追加する

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07
Reopened: 2026-06-22

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

## 解決方法

`src/audio_pipewire.c` の 2 箇所の `pw_init()` 呼び出しに対して戻り値チェックを追加した。

- `audio_enumerate_devices()` (173 行目): `pw_init(NULL, NULL)` の戻り値が負の場合は `-5` を返す
- `audio_session_create()` (385 行目): `pw_init(NULL, NULL)` の戻り値が負の場合は session を free して `NULL` を返す

`pw_init()` は複数回呼び出しても安全（PipeWire 0.3 では参照カウント方式）なため、静的フラグによる 1 回制限は導入せず、既存の呼び出しパターンを維持した。

## reopened にした理由

本 issue の修正（`pw_init()` の戻り値が負ならエラーを返す）を適用した結果、以下のコンパイルエラーが発生した:

```
error: void value not ignored as it ought to be
  173 |     if (pw_init(NULL, NULL) < 0) {
      |         ^~~~~~~~~~~~~~~~~~~
```

原因は issue の根拠そのものが誤っていたためである。`pw_init()` の戻り値型は **存在する全バージョンで `void`** であり、`int` を返すバージョンは一度も存在しない:

- PipeWire 0.2.0（最初期）: `void pw_init(int *argc, char **argv[])`
- PipeWire 0.2.5: `void pw_init(int *argc, char **argv[])`
- PipeWire 0.3.0: `void pw_init(int *argc, char **argv[])`
- PipeWire 1.0.5: `void pw_init(int *argc, char **argv[])`
- PipeWire 1.6.7（最新ドキュメント）: `void pw_init(int *argc, char **argv[])`

以上の調査により、「`pw_init()` は `int` を返し、失敗時は負の errno 値となる」という本 issue の根拠は完全な誤情報（LLM のハルシネーション）であり、元のコード（戻り値チェックなし）が正しかったことが判明した。

### 修正内容

2 箇所の `pw_init()` 呼び出しの戻り値チェックを revert し、元の `pw_init(NULL, NULL);` に戻した。
