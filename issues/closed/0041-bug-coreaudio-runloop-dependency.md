# audio_coreaudio.m AudioQueue コールバックの RunLoop 依存を検証・修正する

- Created: 2026-07-30
- Completed: 2026-10-27
- Branch: feature/fix-coreaudio-runloop
- Polished: {YYYY-MM-DD}

## 目的

`audio_coreaudio.m` で `AudioQueueNewInput` / `AudioQueueNewOutput` に `NULL, kCFRunLoopCommonModes` を渡しているため、呼び出し元スレッドが CFRunLoop をポンプしない場合にコールバックが一切起動しない可能性を検証し、必要に応じて修正する。

## 現状

`src/audio_coreaudio.m` の `audio_coreaudio_session_create` と `audio_coreaudio_playback_session_create` で、AudioQueue の生成に以下を渡している:

```c
AudioQueueNewInput(&session->format, audio_input_callback, session,
                   NULL, kCFRunLoopCommonModes, 0, &session->queue);
```

Apple のドキュメントでは、`inCallbackRunLoop` に `NULL` を渡すとカレントスレッドの RunLoop にコールバックがスケジュールされる。通常の Rust スレッド（`std::thread::spawn` で生成したスレッド）は CFRunLoop をポンプしないため、コールバックが配送されない可能性がある。

現状の検証状況:

- CI の device-test ジョブはデバイス列挙のみ実行し、キャプチャ/再生は検証していない
- `examples/playback_sine.rs` は stdin 待ちのみで RunLoop をポンプしない
- `tests/` にキャプチャ/再生の統合テストはない

コールバックが起動しない場合、キャプチャはフレームを一切配送せず、再生は初期 3 バッファ（約 30ms）で停止する。

## 設計方針

1. まず実機（macOS）で現状の動作を検証する
2. 問題が確認された場合、以下のいずれかで修正する:
   - 専用スレッドで `CFRunLoopRun` を実行する設計に変更する
   - `inCallbackRunLoop` に専用 RunLoop を渡す
   - `inRunLoopMode` に `NULL` を渡し、AudioQueue 内部スレッドでコールバックさせる

## 完了条件

macOS 実機でキャプチャ・再生が正常に動作すること。コールバックが確実に配送されること。

## 解決方法

macOS 実機 (macOS 26.5.2 / arm64) で検証した結果、本 issue が前提とする問題は存在しなかった。コードの変更は行わず、closed にする。

- Apple のドキュメントでは、`AudioQueueNewInput` / `AudioQueueNewOutput` の `inCallbackRunLoop` に `NULL` を渡すと、コールバックは AudioQueue の内部スレッドで呼ばれる。カレントスレッドの RunLoop にはスケジュールされない
- `audio_coreaudio_session_create` と `audio_coreaudio_playback_session_create` が渡している `NULL, kCFRunLoopCommonModes` は、この仕様どおりの使い方である
- リポジトリと同じ引数を使った実機検証では、CFRunLoop を回さない pthread とメインスレッドのどちらでも、入力・出力とも 1 秒に約 100 回のコールバックが配送された
- 対照実験として `inCallbackRunLoop` にカレントスレッドの RunLoop を渡してポンプしない場合、実行中のコールバックは 0 回になり、停止時のフラッシュで 3 回だけ呼ばれた。issue が想定していた症状はこの条件でだけ再現する
- `tests/test_device_timing.rs` の `capture_timestamps_track_audio_time` と `playback_callbacks_track_audio_time` は macOS 実機で成功した。`std::thread::spawn` したスレッドから公開 API を呼ぶ確認でも、キャプチャは 1 秒で 48640 フレーム、再生は 2 秒で 199 回のコールバックが得られた
- macOS の self-hosted ランナーの Device Test でも `playback_callbacks_track_audio_time` と `playback_start_and_restart_after_stop` が成功している
- issue の「現状」にあった CI とテストに関する記述は、統合テストが追加された時点で古くなっている

設計方針に挙げられていた修正案はいずれも不要である。専用スレッドで `CFRunLoopRun` を実行する案はスレッドと停止処理を増やすだけであり、専用 RunLoop を渡す案は逆に RunLoop のポンプを必須にしてしまう。`inRunLoopMode` に `NULL` を渡す案は、ドキュメント上 `kCFRunLoopCommonModes` と等価で挙動が変わらない。
