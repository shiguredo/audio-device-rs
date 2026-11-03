# WASAPI のデバイス列挙とセッション状態管理の不備を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-device-and-session-correctness
- Polished: 2026-11-03

## 目的

WASAPI のデバイス列挙が実際の値を保証できない状態と、セッションの停止・再開始時に状態が残る問題を修正する。

## 現状

- `enumerate_devices_by_type` は `collection.Item` の失敗を `continue`、`GetId` の文字列化失敗を `unwrap_or_default()`、`OpenPropertyStore` の失敗を `"Unknown Device"`、`get_device_format` の失敗を `(2, 48000)` で埋める。実在しない名前・形式が `AudioDeviceList` に流れ、CI の `device_list` の `jq` チェック (`channels > 0 && sample_rate > 0`) も通ってしまう。
- `WasapiDeviceListImpl::enumerate` はフィルタ指定時も入力・出力の両方を列挙し、反対方向の列挙失敗が全体を失敗させる。
- `map_com_error` は `CoCreateInstance` と `GetDevice` にしか適用されていない。`EnumAudioEndpoints` と `GetCount` は原因を捨てて `DeviceAccessDenied` にし、`Item` と `GetId` はエラーを捨ててデバイスをスキップしている。`GetDefaultAudioEndpoint`、`Activate`、`GetMixFormat` なども `map_com_error` を通っていない。
- `get_device_by_id` は ID 指定時にエンドポイントの方向 (入力 / 出力) を検証しない。出力の ID をキャプチャへ渡しても型の検査をしない。
- `WasapiCaptureImpl::stop` / `WasapiPlaybackImpl::stop` は `IAudioClient::Reset` を呼ばず、停止時に `SetEvent` したイベントを非シグナル状態に戻さない。`SetEventHandle` の仕様は「Start を呼ぶ時点でイベントハンドルが非シグナル状態であること」を推奨している。
- ワーカースレッド内の `ComGuard::init_mta()` の失敗を捨てており、COM 未初期化のまま処理を続ける。`stop` は `running` が true のときしか `Stop` を呼ばないため、ワーカーの COM 初期化失敗で `running` を false にすると開始済みのストリームが残る。

## 設計方針

- 列挙全体の COM 呼び出し (`CoCreateInstance` / `EnumAudioEndpoints` / `GetCount`) が失敗した場合はエラーを返し、`map_com_error` で `CO_E_NOTINITIALIZED` を `Error::ComInitFailed` にする。個々のデバイスの取得 (`Item` / `GetId` / `OpenPropertyStore` / `get_device_name` / `get_device_format`) が失敗した場合はそのデバイスをスキップし、既定値を捏造しない。
- フィルタ指定時は対象方向だけを列挙する。フィルタがない場合は入力・出力の両方を列挙し、どちらかの列挙が失敗した場合は全体をエラーにする。
- エラーとして返す COM 呼び出しは `map_com_error` を通し、`CO_E_NOTINITIALIZED` を `Error::ComInitFailed` にする。ワーカースレッドのパケット処理ループ内の COM 呼び出しは、失敗時はそのパケットの処理を中断する現行の扱いを維持する。
- ID 指定時に `device_type` と取得したエンドポイントの方向 (`IMMEndpoint::GetDataFlow`) を検証し、方向が一致しない場合と方向の取得に失敗した場合は `Error::DeviceNotFound` を返す。
- stop 後に `IAudioClient::Reset` を呼び、次回 Start 前に `ResetEvent` でイベントを非シグナル状態へ戻す。`Reset` は `Stop` 後に呼ぶ。
- ワーカースレッドの COM 初期化に失敗した場合は、スレッドを終了して `running` を false にする。`stop` は `running` が false でも、開始済みのスレッドが残っていれば join して `Stop` と `Reset` を呼ぶ。
- 存在しない `device_id` の検出時期とエラー種別は issue 0084 で扱う。本 issue では WASAPI の方向検証だけを対象にする。

## 完了条件

- 列挙結果に捏造値が混ざらない。
- フィルタ指定時に不要な方向を列挙しない。
- 入力 / 出力が一致しない `device_id` を指定すると `Error::DeviceNotFound` になる。
- 停止・再開始で古いイベント状態やバッファ位置が残らない。
- ワーカースレッドの COM 初期化に失敗しても、開始済みのストリームが `stop` で停止する。
- COM 未初期化時に `Error::ComInitFailed` が返る。
- Windows 実機の device-test が成功する。
