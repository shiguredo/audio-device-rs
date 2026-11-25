# PipeWire の列挙が同期要求と main loop の失敗を検査しない

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pipewire-enumeration-errors
- Polished: {YYYY-MM-DD}

## 目的

`audio_pipewire_enumerate_devices` が `pw_core_sync` と `pw_main_loop_run` の
戻り値を捨てているため、同期要求の送信に失敗すると列挙が永久に戻るか、
エラーで終了した main loop を「デバイス 0 件の成功」として返す。呼び出し側が
ハングや空リストの成功を区別できるようにする。

## 現状

`audio_pipewire_enumerate_devices` は `pw_core_sync` の戻り値を
`ctx.pending_sync` にそのまま代入し、`enum_core_done` は
`seq == ctx.pending_sync` のときだけ `pw_main_loop_quit` を呼ぶ。

PipeWire の `pw_core_sync` は `pw_core_method` マクロ経由で呼ばれ、失敗時は
`-ENOTSUP` などの負の値を返す。負の値は `done` の `seq` と一致しないため、
`pw_main_loop_run` が永久に戻らない。`ctx.pending_sync` は 0 で初期化されるため、
戻り値が 0 の場合も「同期要求が発行された」と区別できない。

また `pw_main_loop_run` の戻り値を捨てている。エラーで戻った場合も
`*devices` / `*count` を設定して 0 を返すため、Rust 側の
`FfiDeviceListImpl::enumerate` は列挙成功として扱う。

0058 はサーバ切断時の `done` 未到着とタイムアウトを扱っている。本 issue は
同じ関数のうち、同期要求そのものの失敗と main loop の失敗を扱う。

## 設計方針

- `ctx.pending_sync` を「未設定」を表す `-1` で初期化する
- `EnumerateContext` に `done_received` を追加し、`enum_core_done` が
  一致する `done` を受けたときだけ真にする
- `pw_core_sync` の戻り値が負なら、列挙済みの `ctx.devices` を
  `audio_pipewire_free_devices` で解放し、`AudioPipeWireResult` の負の値を返す
- `enum_core_done` は `ctx.pending_sync >= 0 && seq == ctx.pending_sync` の
  ときだけ `pw_main_loop_quit` を呼ぶ
- `pw_main_loop_run` の戻り値を保持し、負なら `ctx.devices` を解放して
  負の値を返す。0 以上でも `done_received` が偽なら、列挙が完了していない
  ものとしてエラーにする
- エラー終了時は `audio_pipewire_free_devices` を呼び、Rust 側の
  `FfiDeviceListImpl::enumerate` が `ret < 0` で受け取れるようにする
- 0058 の core error listener とタイムアウト導入とは独立に実装できるが、
  同じ `audio_pipewire_enumerate_devices` を変更するため、ブランチの競合に
  注意する

## 完了条件

- `pw_core_sync` が負の値を返した場合、`audio_pipewire_enumerate_devices` が
  有限時間で負の値を返し、列挙済みデバイスを解放する
- `pw_main_loop_run` が負の値を返した場合、同じく負の値を返して解放する
- `pw_core_sync` の戻り値が 0 の場合に、`done` 到着前の `quit` で空リストを
  成功として返さない。`done_received` が偽のまま main loop が戻った場合も
  エラーにする
- `cargo test --workspace --no-default-features --features default-pipewire` が通る
