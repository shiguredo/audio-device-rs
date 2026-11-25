# WASAPI がデバイス無効化を検出せず無音のまま動作し続ける

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-device-invalidation
- Polished: {YYYY-MM-DD}

## 目的

USB マイクの抜去や既定デバイスの変更で WASAPI が恒久的なエラーを返すと、
現在の実装はエラーを捨てて待機ループに戻る。`running` は真のまま、
コールバックは来なくなり、利用者は無音になった理由を取得できない。
恒久的なエラーを検出してワーカーを終了させ、異常を観測できるようにする。

## 現状

`capture_thread_func` は `IAudioCaptureClient::GetNextPacketSize` が
エラーを返すと内側のパケットループを抜けるだけで、10 ms 待機の外側ループに
戻る。`playback_thread_func` は `IAudioClient::GetCurrentPadding` が
エラーを返すと `continue` で同じループを繰り返す。

USB デバイスの抜去や既定デバイス変更では `AUDCLNT_E_DEVICE_INVALIDATED`、
`AUDCLNT_E_RESOURCES_INVALIDATED`、`AUDCLNT_E_SERVICE_NOT_RUNNING` などが
返り続ける。`running` は真のままなので `stop` を呼ぶまでワーカーは回り続け、
コールバックは呼ばれない。`AudioCaptureSession` / `AudioPlaybackSession` から
異常を取得する API もない。

0060 は「パケット処理ループ内の COM 呼び出しは失敗時にそのパケットの処理を
中断する現行の扱いを維持する」としているが、恒久的なエラーの検出と通知は
対象外である。0068 はバックエンドのエラーコードの公開エラーへの写像を扱う。

## 設計方針

- `GetNextPacketSize` / `GetCurrentPadding` / `GetBuffer` のエラーを、
  一時的なエラーと恒久的なデバイス無効化に分類する
- 恒久的なエラーを受けたら `running` を 0 にし、ワーカーを終了する。
  10 ms 待機で同じエラーを再試行し続けない
- ワーカーが観測したエラーコードを保持する。状態の所有者を 1 箇所に
  定める方針に合わせ、ワーカーと `stop` の共有は `Arc<AtomicI32>` などの
  単純な値に限定する。`Mutex` は使わない
- 利用者への通知方法を、0068 のエラー写像と整合する形で決める。
  `AudioCaptureSession` / `AudioPlaybackSession` にエラー取得メソッドを
  追加するか、`stop` の戻り値を `Result` にする。公開 API を変更する場合は
  `CHANGES.md` の `[CHANGE]` と rustdoc を更新する
- 通知しない方を選ぶ場合は、デバイス無効化後にコールバックが来なくなり、
  `stop` して `start` し直す必要があることを rustdoc に明記する
- 一時的なエラーでは従来どおりパケット処理を中断し、次のイベントを待つ

## 完了条件

- デバイス無効化を表す HRESULT を受けたワーカーが `running` を 0 にして
  終了し、同じエラーで待機ループを回り続けない
- 利用者が異常を取得できる API があるか、通知しないことが rustdoc に
  明記されている
- 公開 API を変更した場合、`CHANGES.md` と rustdoc が更新されている
- Windows 実機でデバイスを取り外すテスト、またはコードレビューで
  HRESULT の分類と終了経路を確認する
- `cargo clippy --target x86_64-pc-windows-msvc` が通る
