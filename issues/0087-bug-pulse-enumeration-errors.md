# PulseAudio の列挙が eol < 0 と sink 操作の生成失敗を成功として扱う

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pulse-enumeration-errors
- Polished: {YYYY-MM-DD}

## 目的

`audio_pulse_enumerate_devices` が、一覧取得コールバックのエラー
(`eol < 0`) で永久に待ち、`pa_context_get_sink_info_list` の生成失敗を
「source のみの成功」として返す。列挙の失敗を呼び出し側が検出できるようにする。

## 現状

`source_info_callback` / `sink_info_callback` は `eol > 0` のときだけ
`ctx->done` を進める。PulseAudio の公式ドキュメントは、一覧取得の
コールバックについて「エラー時は情報構造体なしで `eol` が負の値で呼ばれる」と
定義している。`eol < 0` では `info` が NULL のためコールバックは何もせず戻り、
`done` は進まない。

待機ループは `pa_mainloop_iterate(mainloop, 1, &ret)` でブロックする。
コールバックが呼ばれた時点でその操作のイベントは尽きているため、サーバが
応答していても `done` が永久に進まず、列挙が戻らない。

また `pa_context_get_sink_info_list` が NULL を返した場合、
`audio_pulse_enumerate_devices` は source の結果だけを設定して 0 を返す。
`AudioDeviceList::enumerate_output_pulse()` は出力デバイス 0 件の成功として
扱う。source 側の同じ失敗は負の値を返しており、非対称である。

0059 はサーバ無応答時のタイムアウトと、部分的なリストを返さない方針を
扱っている。本 issue は、サーバが応答している場合の `eol < 0` と、
操作そのものの生成失敗を扱う。

## 設計方針

- `EnumerateContext` にエラー状態を表すフィールドを追加する
- `source_info_callback` / `sink_info_callback` は `eol < 0` を受けたら
  エラー状態を記録する。`done` の判定とエラー状態を分け、待機ループが
  エラーで抜けられるようにする
- source / sink の待機ループは、コンテキストの失敗に加えてエラー状態も
  確認して抜ける
- エラー状態で抜けた場合は、列挙済みの `enum_ctx.devices` を
  `audio_pulse_free_devices` で解放し、負の値を返す
- `pa_context_get_sink_info_list` が NULL を返した場合も、source の結果を
  返さずに列挙済みデバイスを解放して負の値を返す
- `pa_operation_unref` と `pa_context_disconnect` / `pa_context_unref` /
  `pa_mainloop_free` の既存の後始末を、新しいエラー経路でも同じ順序で行う
- 0059 のタイムアウト導入とは独立に実装できるが、同じ
  `audio_pulse_enumerate_devices` を変更するため、ブランチの競合に注意する

## 完了条件

- 一覧取得コールバックが `eol < 0` で呼ばれた場合、列挙が有限時間で負の値を
  返し、列挙済みデバイスを解放する
- `pa_context_get_sink_info_list` が NULL を返した場合、source のみの成功を
  返さず、負の値を返す
- `AudioDeviceList::enumerate_output_pulse()` が、sink 列挙の失敗を
  空リストの成功として返さない
- `cargo test --workspace --no-default-features --features default-pulse` が通る
