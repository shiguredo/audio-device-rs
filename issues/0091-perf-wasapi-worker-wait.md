# WASAPI ワーカーの 10 ms タイムアウト起床と WAIT_FAILED の再試行を修正する

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-worker-wait
- Polished: {YYYY-MM-DD}

## 目的

WASAPI のキャプチャ・再生ワーカーが、イベント駆動であるにもかかわらず
10 ms のタイムアウトで待機し、毎秒約 100 回起床している。停止は `SetEvent` で
ワーカーを起こせるため、タイムアウトは不要である。あわせて
`WAIT_FAILED` / `WAIT_ABANDONED` を `continue` で握り潰しているため、
イベントハンドルが無効になると 100% CPU のビジーループになる。待機方法と
異常系の扱いを修正する。

## 現状

`capture_thread_func` と `playback_thread_func` は、どちらも
`WaitForSingleObject(event_handle, 10)` で 10 ms のタイムアウト付き待機を
行う。`WasapiCaptureImpl::stop` / `WasapiPlaybackImpl::stop` は
`SetEvent(session.event_handle)` でワーカーを起こしているため、タイムアウトは
停止要求の伝達には使われていない。

待機結果が `WAIT_OBJECT_0` でも `WAIT_TIMEOUT` でもない場合、現在の実装は
`continue` で即座に次の待機へ進む。イベントハンドルが閉じられていると
`WaitForSingleObject` は `WAIT_FAILED` を即座に返すため、待機なしのループに
なる。0085 の二重起動があると、detach されたスレッドが閉じたハンドルに対して
この経路に入る。

無音やデバイス無効化でエンジンがイベントを通知しない状況でも、10 ms ごとに
起床し続けるため、バッテリー駆動端末で消費電力が増える。

## 設計方針

- 待機を `WaitForSingleObject(event_handle, INFINITE)` に変更する。停止は
  既存どおり `SetEvent` で行う
- `WAIT_FAILED` / `WAIT_ABANDONED` を受けたら `continue` せず、ワーカーを
  終了する。エラーコードを保持できる場合は、0089 のデバイス無効化と同じ
  経路で利用者へ通知する
- イベントハンドルの有効期間を、ワーカーの終了（join）より後に閉じることを
  保証する。0085 の join 修正と順序を揃える
- タイムアウトを残す必要がある場合は、監視目的でも 100 ms 以上に延ばし、
  10 ms 周期の起床をなくす
- 停止要求から join までの時間が、現行の 10 ms タイムアウトに依存しないことを
  確認する

## 完了条件

- キャプチャ・再生のワーカーが、イベント待機で 10 ms ごとに起床しない
- `WAIT_FAILED` / `WAIT_ABANDONED` を受けたワーカーがビジーループに入らず、
  終了する
- `stop` が `SetEvent` でワーカーを起こし、join まで有限時間で戻る
- Windows 実機で停止・再開始を繰り返すテスト、またはコードレビューで
  待機結果の分岐とハンドルの有効期間を確認する
- `cargo clippy --target x86_64-pc-windows-msvc` が通る
