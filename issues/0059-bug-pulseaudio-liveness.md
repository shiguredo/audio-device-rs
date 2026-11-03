# PulseAudio の無期限ブロックと非同期障害の未通知を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pulseaudio-liveness
- Polished: 2026-11-03

## 目的

PulseAudio でサーバが応答しない場合に公開 API が無期限にブロックする問題と、PulseAudio のセッション開始後に非同期障害が起きても公開 API からそのことが分からない問題を修正する。非同期障害については、本 issue では公開 API を増やさず、rustdoc に通知されないことを明記する。

## 現状

- `start_context` の READY 待ち (`pa_threaded_mainloop_wait`) と `wait_stream_ready` の READY 待ち (`pa_threaded_mainloop_wait`) にタイムアウトが無く、サーバが応答しない場合に `AudioCapture::start` / `AudioPlayback::start` が永久に戻らない。`audio_pulse.h` には「start は無期限にブロックする」と書かれているが、利用者が読む rustdoc には無い。
- `audio_pulse_enumerate_devices` も READY 待ち (`while (pa_context_get_state(context) != PA_CONTEXT_READY)`) と `done` 待ち (`while (enum_ctx.done < 1)` / `while (enum_ctx.done < 2)`) にタイムアウトが無く、ヘッダにも rustdoc にも記載が無い。コンテキストが `PA_CONTEXT_FAILED` / `PA_CONTEXT_TERMINATED` になった場合は待機ループを抜けるが、その場合も部分的なデバイスリストを返して 0 を返すため、呼び出し側は失敗を区別できない。
- 開始後の `stream_state_callback` / `playback_stream_state_callback` は `PA_STREAM_FAILED` / `PA_STREAM_TERMINATED` で mainloop をシグナルするだけで、開始完了後に状態を確認する経路が無い。サーバ断やデバイス取り外しの後も `running` が真のまま残り、コールバックが永久に来なくなる。
- `AudioCaptureSession` / `AudioPlaybackSession` の doc には、無期限にブロックする条件も非同期障害が通知されないことも書かれていない。
- PipeWire の開始後の `core_error` も利用者へ伝わらないが、これは issue 0058 で扱う。本 issue では PulseAudio だけを対象にする。

## 設計方針

- start と列挙に 10 秒のタイムアウトを導入する。10 秒は `audio_pipewire.c` の `STREAM_STATE_TIMEOUT_SEC` と同じ値とし、経過時間は `CLOCK_MONOTONIC` で判定する。
- PulseAudio 16.1 (Ubuntu 24.04 の `libpulse-dev`) には `pa_threaded_mainloop_timed_wait` が無い。start 側は `pa_threaded_mainloop_get_api` で取得した `pa_mainloop_api` の `pa_time_event` を使って待機中の mainloop を 1 秒間隔でシグナルし、`CLOCK_MONOTONIC` の経過時間が 10 秒を超えたら負の値を返す。列挙側は `pa_mainloop_prepare` に残り時間を渡し、`pa_mainloop_poll` / `pa_mainloop_dispatch` を繰り返して、同じく 10 秒を超えたら負の値を返す。`pa_time_event` の待機はシステム時刻の影響を受けるため、システム時刻の後退やサスペンドがある環境では 10 秒の上限は保証されない。
- タイムアウト用の `pa_time_event` は待機終了時に解放する。
- `start_context` だけでなく `wait_stream_ready` にも同じタイムアウトを適用する。タイムアウト時は `start_context` の失敗パスと同じく mainloop / context / stream を破棄する。
- 列挙のタイムアウト時は、列挙済みの `ctx.devices` を `audio_pulse_free_devices` で解放してから負の値を返す。コンテキストの `PA_CONTEXT_FAILED` / `PA_CONTEXT_TERMINATED` および `pa_mainloop_prepare` / `pa_mainloop_poll` / `pa_mainloop_dispatch` の失敗も、部分的なリストを返さず負の値にする。
- タイムアウトの戻り値は既存の失敗経路と同じ負の値とし、Rust 側では既存の `Error::SessionStartFailed` / `Error::DeviceAccessDenied` に写像する。タイムアウトを区別する新しい `Error` variant は issue 0068 で扱う。
- `audio_pulse.h` の「start は ... 無期限にブロックする」というコメントを、約 10 秒でタイムアウトする仕様に更新する。`audio_pulse_enumerate_devices` にも同じ待機時間を記載する。
- PulseAudio の非同期障害は API では通知しない。`AudioCaptureSession` / `AudioPlaybackSession` の doc に、開始後にサーバ断・デバイス取り外しが起きた場合、コールバックは来なくなり、サーバ復旧後に `stop` して `start` し直すことで復帰できることを明記する。共通 API の追加が必要な場合は issue 0058 で扱い、本 issue では公開 API を変更しない。rustdoc 全体の契約整理は issue 0072 で扱う。
- `AudioCapture::start` / `AudioPlayback::start` と `AudioDeviceList::enumerate*` の rustdoc に、PulseAudio ではサーバが応答しない場合に約 10 秒でエラーを返すことを記載する。本 issue では PulseAudio の待機時間と非同期障害に関する記述だけを追加する。
- `CHANGES.md` の `## develop` にある既存の `[UPDATE]` エントリ「PulseAudio のセッション開始はサーバが応答しない場合に無期限にブロックすることをドキュメントに記載する」は、タイムアウト導入後は事実と異なる。これを削除し、`[FIX]` セクションに「PulseAudio のセッション開始とデバイス列挙がサーバ無応答時に約 10 秒でタイムアウトするようにする」を追加する。ドキュメント変更自体は `shiguredo-changelog` の規約に従い、CHANGES の別エントリにはしない。

## 完了条件

- PulseAudio サーバが応答しない場合、`AudioCapture::start` / `AudioPlayback::start` が約 10 秒 (待機の粒度により最大約 11 秒) 以内に `Error::SessionStartFailed` を返す。`start_context` と `wait_stream_ready` の両方の待機が対象である。
- PulseAudio サーバが応答しない場合、`AudioDeviceList::enumerate_pulse()` / `enumerate_input_pulse()` / `enumerate_output_pulse()` が約 10 秒 (待機の粒度により最大約 11 秒) 以内に `Error::DeviceAccessDenied` を返す。タイムアウト時は列挙済みのデバイスが解放される。
- `audio_pulse.h` と公開 rustdoc に、PulseAudio の start と列挙のタイムアウト、および開始後の非同期障害が通知されないことが記載されている。
- `CHANGES.md` の `## develop` に、PulseAudio のタイムアウトを `[FIX]` として記載するエントリがある。
- `cargo test --workspace` が通り、CI の Ubuntu (pulse) ジョブと `device-test` ジョブが成功する。
