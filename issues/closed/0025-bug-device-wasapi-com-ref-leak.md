# device_wasapi.rs の COM 初期化カウンタ未解放

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-10-19
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

設計方針の「`WasapiCaptureImpl` / `WasapiPlaybackImpl` が `ComGuard` をメンバーとして保持する」案は採用しなかった。公開型が `Send` であるため、オブジェクトを別スレッドへ移動して drop すると、`CoUninitialize` を初期化したスレッドで呼べず、ガードが不完全になるためである。

- `src/runtime.rs` に `initialize_thread()` と `ThreadGuard` を追加した。Windows では `CoInitializeEx` で MTA を初期化し、`ThreadGuard` の drop で `CoUninitialize` を呼ぶ。macOS / Linux では何もしない
- `enumerate_devices_by_type`、`get_device_by_id`、`WasapiCaptureImpl` / `WasapiPlaybackImpl` の `new` / `start` / `stop` / `Drop` から COM の初期化を削除した。ライブラリを使うスレッドごとに、そのスレッドの先頭で `initialize_thread()` を呼ぶ契約に変更した
- ライブラリが生成するスレッドでは、`spawn` の直前に `ComGuard::init_mta()` を置いた。ガードはスレッド関数の引数 (COM インターフェース) が解放された後に drop される
- 呼び出し元スレッドが COM を初期化していない場合は、`CoCreateInstance` が返す `CO_E_NOTINITIALIZED` を検出して `Error::ComInitFailed` に変換するようにした
- `README.md` と `examples/` を新しい使い方に更新し、`CHANGES.md` の `## develop` に `[CHANGE]` と `[FIX]` を追記した

`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` がすべてパスすることを確認した。実機 (Windows) でデバイス列挙の繰り返し、キャプチャと再生の開始・停止・破棄、別スレッドでの破棄を行い、COM の初期化カウンタが増えないこと、`initialize_thread()` のガードを drop するとカウンタが 0 に戻ることを確認した。

## 後方互換

Windows では利用者がスレッドごとに `initialize_thread()` を呼ぶ必要があるため、後方互換のない変更になる。`init_com_mta()` と `get_device_by_id` は `pub(crate)` のままで、公開 API には `initialize_thread()` と `ThreadGuard` を追加した。
