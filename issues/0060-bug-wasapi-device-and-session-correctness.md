# WASAPI のデバイス列挙とセッション状態管理の不備を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-device-and-session-correctness
- Polished: {YYYY-MM-DD}

## 目的

WASAPI のデバイス列挙が実際の値を保証できない状態と、セッションの停止・再開始時に状態が残る問題を修正する。

## 現状

- `enumerate_devices_by_type` は `collection.Item` の失敗を `continue`、`GetId` の文字列化失敗を `unwrap_or_default()`、`OpenPropertyStore` の失敗を `"Unknown Device"`、`get_device_format` の失敗を `(2, 48000)` で埋める。実在しない名前・形式が `AudioDeviceList` に流れ、CI の `device_list` の `jq` チェック (`channels > 0 && sample_rate > 0`) も通ってしまう。
- `WasapiDeviceListImpl::enumerate` はフィルタ指定時も入力・出力の両方を列挙し、反対方向の列挙失敗が全体を失敗させる。
- `map_com_error` は `CoCreateInstance` と `GetDevice` にしか適用されておらず、`EnumAudioEndpoints` / `GetCount` / `Item` / `GetId` は原因を捨てて `DeviceAccessDenied` にしている。
- `get_device_by_id` は ID 指定時にエンドポイントの方向 (入力 / 出力) を検証しない。出力の ID をキャプチャへ渡しても型の検査をしない。
- `WasapiCaptureImpl::stop` / `WasapiPlaybackImpl::stop` は `IAudioClient::Reset` を呼ばず、停止時に `SetEvent` したイベントを非シグナル状態に戻さない。`SetEventHandle` の仕様は「Start を呼ぶ時点でイベントハンドルが非シグナル状態であること」を求めている。
- ワーカースレッド内の `ComGuard::init_mta()` の失敗を捨てており、COM 未初期化のまま処理を続ける。
- `device_id` の不正検出時期がバックエンドで不統一である (CoreAudio は `create`、PulseAudio は `start`、PipeWire は約 10 秒のタイムアウト、WASAPI は `start`)。同じ誤用の挙動がプラットフォームで異なる。

## 設計方針

- 取得できなかった項目はデバイスをスキップするかエラーとして返し、既定値を捏造しない。フィルタ指定時は対象方向だけを列挙する。
- COM を呼ぶ全箇所で `map_com_error` を通す。
- ID 指定時に `device_type` と取得したエンドポイントの方向を検証する。
- stop 後に `IAudioClient::Reset` を呼び、次回 Start 前にイベントを非シグナル状態へ戻す (`ResetEvent` など)。
- ワーカースレッドの COM 初期化失敗時はスレッドを終了し、`running` を false にする。
- `device_id` の不正は可能な限り `new` または `start` の早い段階で検出し、バックエンド間でエラー種別を揃える。

## 完了条件

- 列挙結果に捏造値が混ざらない。
- フィルタ指定時に不要な方向を列挙しない。
- 停止・再開始で古いイベント状態やバッファ位置が残らない。
- `device_id` の誤用がバックエンド間で同じ種類のエラーになる。
- Windows 実機の device-test が成功する。
