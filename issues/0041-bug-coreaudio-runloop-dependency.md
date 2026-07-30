# audio_coreaudio.m AudioQueue コールバックの RunLoop 依存を検証・修正する

- Created: 2026-07-30
- Completed: {YYYY-MM-DD}
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

検証結果に応じて決定する。問題がない場合は、その旨を記録して closed にする。問題がある場合は上記設計方針のいずれかで修正し、macOS 実機で動作確認する。
