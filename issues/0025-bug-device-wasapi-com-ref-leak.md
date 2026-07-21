# device_wasapi.rs の COM 初期化カウンタ未解放

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-device-wasapi-com-ref-leak
- Polished: 2026-07-21

## 目的

`src/device_wasapi.rs` の `init_com_mta()` が `CoInitializeEx` を呼ぶが、呼び出し元スレッドで対応する `CoUninitialize` が一度も呼ばれない問題を修正する。

## 優先度根拠

デバイスの生成・破棄を繰り返す長期実行アプリケーションで COM 初期化カウンタが増え続ける。

## 現状

src/device_wasapi.rs:36-45:

```rust
pub(crate) fn init_com_mta() -> Result<()> {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
        if hr.is_ok() {
            Ok(())
        } else {
            Err(Error::ComInitFailed)
        }
    }
}
```

`CoInitializeEx` は成功時（`S_OK`）も既に初期化済み時（`S_FALSE`）もスレッドの COM 初期化カウンタをインクリメントする。いずれの場合も対応する `CoUninitialize` が必要だが、呼び出し元で一度も呼ばれていない。

呼び出し箇所:
- `device_wasapi.rs:95`（`enumerate_devices_by_type` 内）— `WasapiDeviceListImpl::enumerate()` は Input + Output で 2 回呼ぶため 2 回インクリメント
- `device_wasapi.rs:208`（`get_device_by_id` 内）— `WasapiCaptureImpl::new()` / `WasapiPlaybackImpl::new()` から 1 回呼ばれる

なお、ワーカースレッド（`capture_wasapi.rs:240,326`、`playback_wasapi.rs:269,339`）は `CoInitializeEx`/`CoUninitialize` の対が正しく取れている。問題は呼び出し元スレッド。

## 設計方針

`init_com_mta()` の呼び出し元で RAII ガードを使い、スコープ終了時に `CoUninitialize` を呼ぶ設計にする。RAII ガードは `device_wasapi.rs` 内に `pub(crate)` struct として定義する（例: `ComGuard`）。`init_com_mta()` は `ComGuard` を返すようにシグネチャを変更する。

プロセス全体で 1 回だけ初期化する案は、スレッドが COM を使う前に `CoInitializeEx` で参加を宣言する必要がある COM の仕様上不適切。明示的 `CoUninitialize` 案はエラーパスでの解放漏れリスクがあるため RAII ガードが最も堅牢。

### ガードのライフサイクル制約

`get_device_by_id` の呼び出し元（`WasapiCaptureImpl::new()` / `WasapiPlaybackImpl::new()`）は、戻り値の `IMMDevice` を受け取った後も同じスレッドで COM 操作（`device.Activate()`、`audio_client.GetService()` 等）を継続する。さらに `Drop` 実装（`capture_wasapi.rs:214-222`、`playback_wasapi.rs:241-249`）も `audio_client.Stop()` 等の COM 操作を呼ぶ。

したがって `get_device_by_id` 内でガードを drop してはならない。ガードは呼び出し元が保持し、COM オブジェクトの全ライフサイクルをカバーする必要がある。

- `enumerate_devices_by_type`: 全 COM オブジェクトが関数内で生成・破棄されるため、関数内でガードを束縛して問題ない
- `get_device_by_id`: ガードを呼び出し元に返す必要がある。`WasapiCaptureImpl` / `WasapiPlaybackImpl` がフィールドとして `ComGuard` を保持する

## 完了条件

- `init_com_mta()` の呼び出し元で COM 初期化カウンタが RAII ガードにより正しく管理される
- `WasapiCaptureImpl` / `WasapiPlaybackImpl` の `Drop` 時に COM 操作が安全に呼べる（ガードが COM オブジェクトより先に drop されない）
- `cargo clippy --target x86_64-pc-windows-msvc` が通る（WASAPI は Windows 専用のため、macOS 上の `cargo test` ではこのコードパスは検証されない。クロスコンパイルでの静的検証を行う）
- 実機テストが不可能な場合はワーカースレッドの既存パターン（`capture_wasapi.rs:240,326`）との差分対照によるコードレビューで担保する

## 解決方法

1. `device_wasapi.rs` に RAII ガード `ComGuard` を追加する。`Drop` で `unsafe { CoUninitialize() }` を呼ぶ。`CoUninitialize` は `CoInitializeEx` を呼んだのと同じスレッドで呼ぶ必要があるため、`ComGuard` は `PhantomData<*const ()>` を持たせて `!Send` とする（スレッドアフィニティの文書化）
2. `init_com_mta()` の戻り値を `Result<()>` から `Result<ComGuard>` に変更する
3. `enumerate_devices_by_type`（95 行目）では `let _com = init_com_mta()?;` のようにガードを関数内で束縛する。全 COM オブジェクトが関数内で完結するため問題ない
4. `get_device_by_id`（208 行目）では `init_com_mta()` を呼び出し、ガードを戻り値に含める。シグネチャを `Result<(ComGuard, IMMDevice)>` に変更する
5. `WasapiCaptureImpl` / `WasapiPlaybackImpl` に `_com_guard: ComGuard` フィールドを追加し、`new()` で `get_device_by_id` から受け取ったガードを保持する。フィールドの宣言順序に注意: `_com_guard` は COM オブジェクト（`session`）より **後** に宣言する。将来 `Drop::drop()` の実装が変わった場合でも、フィールド drop 時の COM Release が `CoUninitialize` より先に呼ばれることを保証するため

## 後方互換

`init_com_mta()` と `get_device_by_id` は `pub(crate)` であり公開 API に影響なし。内部呼び出し側（`enumerate_devices_by_type`、`get_device_by_id`、`WasapiCaptureImpl`、`WasapiPlaybackImpl`）の構造が変わるが、外部からは観測できない
