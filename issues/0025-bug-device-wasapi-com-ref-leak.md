# device_wasapi.rs の COM 参照リーク

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-device-wasapi-com-ref-leak
- Polished: {YYYY-MM-DD}

## 目的

`src/device_wasapi.rs` の `init_com_mta()` が `CoInitializeEx` を呼ぶが、呼び出し元スレッドで対応する `CoUninitialize` が一度も呼ばれない問題を修正する。

## 優先度根拠

デバイスの生成・破棄を繰り返す長期実行アプリケーションで COM 参照カウントが増え続ける。

## 現状

src/device_wasapi.rs:37-47:

```rust
pub(crate) fn init_com_mta() -> Result<()> {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
        // CoUninitialize の対応呼び出しがどこにもない
        if hr.is_ok() { Ok(()) } else { Err(Error::ComInitFailed) }
    }
}
```

- `WasapiDeviceListImpl::enumerate()` は `enumerate_devices_by_type` を 2 回呼ぶ（Input + Output）→ 2 回の `CoInitializeEx`、0 回の `CoUninitialize`
- `WasapiCaptureImpl::new()` / `WasapiPlaybackImpl::new()` は `get_device_by_id` を呼ぶ → 1 回の `CoInitializeEx`、0 回の `CoUninitialize`

なお、ワーカースレッド（`capture_thread_func`、`playback_thread_func`）は `CoInitializeEx`/`CoUninitialize` の対が正しく取れている。問題は呼び出し元スレッド。

## 完了条件

- `init_com_mta()` の呼び出し元で COM 参照カウントが正しく管理される
- `cargo clippy` / `cargo test` が通る

## 解決方法

RAII ガードを導入するか、`init_com_mta()` の呼び出し元で `CoUninitialize` を呼ぶ。あるいは、プロセス全体で一度だけ MTA 初期化する設計に変更し、参照カウントの増減を管理する。
