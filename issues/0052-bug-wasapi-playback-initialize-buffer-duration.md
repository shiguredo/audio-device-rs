# WASAPI 再生の Initialize で共有モードのバッファ長指定を 0 にする

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-playback-initialize-buffer-duration
- Polished: {YYYY-MM-DD}

## 目的

WASAPI 再生の `IAudioClient::Initialize` が共有モード + イベント駆動の仕様に違反している状態を修正する。

## 現状

- `playback_wasapi.rs` の `WasapiPlaybackImpl::new` は `AUDCLNT_SHAREMODE_SHARED` と `AUDCLNT_STREAMFLAGS_EVENTCALLBACK` を指定しているのに、`hnsBufferDuration` に `100_000` (100 ナノ秒単位で 10 ms)、`hnsPeriodicity` に 0 を渡している。
- Microsoft Learn の `IAudioClient::Initialize` の Remarks は「For a shared-mode stream that uses event-driven buffering, the caller must set both hnsPeriodicity and hnsBufferDuration to 0.」と明記しており、この指定は MUST 違反である。バッファ長はオーディオエンジンの周期で決まる。
- コードコメントの「100 ナノ秒単位で 10ms を指定する」は仕様と逆の説明になっている。同じ実装のキャプチャ側 (`capture_wasapi.rs`) は 0 / 0 を渡しており、再生側だけが逸脱している。
- 現時点で Windows 実機での起動失敗は確認していないが、仕様外の要求であり、ドライバやエンジンの周期によって挙動保証が無い。

## 設計方針

- `hnsBufferDuration` を 0 にする。
- コメントを「共有モードのイベント駆動では 0 を渡し、バッファ長はエンジンが決める。実際の長さは `GetBufferSize` で取得する」に修正する。
- 実機の device-test で再生開始とコールバックが引き続き成功することを確認する。

## 完了条件

- WASAPI 再生の `Initialize` が `hnsBufferDuration = 0` / `hnsPeriodicity = 0` で呼ばれる。
- コメントが仕様と一致している。
- Windows 実機の device-test で再生系テストが成功する。
- `cargo fmt` / `cargo clippy --workspace --all-targets` が通る。
