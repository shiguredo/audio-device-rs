# PulseAudio の無期限ブロックと非同期障害の未通知を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pulseaudio-liveness
- Polished: {YYYY-MM-DD}

## 目的

PulseAudio でサーバが応答しない場合に公開 API が無期限にブロックする問題と、セッション開始後の非同期障害が利用者へ伝わらない問題を修正する。

## 現状

- `start_context` の READY 待ち (`pa_threaded_mainloop_wait`) にタイムアウトが無く、サーバが応答しない場合に `AudioCapture::start` / `AudioPlayback::start` が永久に戻らない。`audio_pulse.h` には「start は無期限にブロックする」と書かれているが、利用者が読む rustdoc には無い。
- `audio_pulse_enumerate_devices` も READY 待ち (`while (pa_context_get_state(context) != PA_CONTEXT_READY)`) と `done` 待ち (`while (enum_ctx.done < 1)` / `while (enum_ctx.done < 2)`) にタイムアウトが無く、ヘッダにも rustdoc にも記載が無い。
- 開始後の `stream_state_callback` / `playback_stream_state_callback` は `PA_STREAM_FAILED` / `PA_STREAM_TERMINATED` で mainloop をシグナルするだけで、開始完了後に状態を確認する経路が無い。サーバ断やデバイス取り外しの後も `running` が真のまま残り、コールバックが永久に来なくなる。PipeWire も同様に、開始後の `core_error` を利用者へ伝える手段が無い。
- `AudioCaptureSession` / `AudioPlaybackSession` の doc には、無期限にブロックする条件も非同期障害が通知されないことも書かれていない。

## 設計方針

- start と列挙にタイムアウトを導入する。PulseAudio は `pa_threaded_mainloop_timed_wait` などを利用し、経過時間は `CLOCK_MONOTONIC` で判定する。
- 非同期障害を記録し、stop または状態取得 API から取得できるようにする。API を増やさない場合は rustdoc に「開始後の非同期障害は通知されない」と明記する。
- `CHANGES.md` の「無期限にブロックすることをドキュメントに記載する」という記述を、公開 rustdoc の記載と一致させる。

## 完了条件

- PulseAudio サーバが応答しない場合、start と列挙が有限時間でエラーを返す。
- 開始後の非同期障害を利用者が観測できるか、通知されないことが doc に明記されている。
- `cargo test --workspace` が通り、既存の実機テストが成功する。
