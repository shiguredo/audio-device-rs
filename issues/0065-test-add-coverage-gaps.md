# テストカバレッジの不足を解消する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/add-test-coverage-gaps
- Polished: {YYYY-MM-DD}

## 目的

公開 API と FFI 境界の未検証箇所を洗い出し、リグレッションを検出できるテストを追加する。

## 現状

- `src/capture_ffi.rs` の `frame_callback` には単体テストが 1 件も無い。同じ境界の `playback_callback` には NULL・境界値・変換・無音のテストが 14 件あり、非対称である。`frame_callback` の NULL 引数、`frames <= 0`、`channels <= 0`、未知フォーマット、サイズ計算のオーバーフロー分岐が未検証。
- `tests/session_lifecycle.rs` の `wait_for_process` は `recv_timeout` のタイムアウトを無視して続行するため、`start()` が `Ok` を返してもコールバックが一度も来ない実装を検出できない。10 サイクルの再開始テストも最終的な `assert_no_process_after_stop` も、コールバックが 0 回なら自明に通る。
- `tests/session_lifecycle.rs` の `input_device_count` / `output_device_count` と `tests/test_device_timing.rs` の列挙ヘルパーは件数または空判定しか見ておらず、返った全デバイスの `device_type()` が期待どおりかを検証していない。WASAPI のフィルタ不具合のような問題を検出できない。
- `sample_rate: 0` / `channels: 0` の既定値クランプを確認するテストが無い。既定値の実装がバックエンドごとに存在するため回帰しやすい。
- `AudioDevice::name()` / `unique_id()` が `Ok` かつ非空であること、`Err(Error::NullPointer(...))` 経路が実機テストで検証されていない。
- examples と README のコード例はコンパイル検証の対象になっていない。

## 設計方針

- `capture_ffi.rs` に `playback_ffi.rs` と対称のテストを追加する。
- `session_lifecycle` で少なくとも 1 サイクルはコールバック到着を必須にし、デバイス 0 件のスキップとコールバック不来を区別する。
- 列挙結果の `device_type()` 全件一致と、全デバイス列挙との和集合・重複を検証する。
- 既定値クランプの実機テストを追加する。
- デバイス名・ID の `Ok` かつ非空を検証する。
- README の代表サンプルを doctest にするか examples から生成する。

## 完了条件

- `frame_callback` の主要分岐にテストがある。
- コールバックが来ない実装が `session_lifecycle` で失敗する。
- デバイス列挙の方向と既定値が検証されている。
- 追加したテストがローカルと CI で通る。
