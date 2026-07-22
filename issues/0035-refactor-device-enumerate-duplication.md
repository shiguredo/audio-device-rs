# device.rs の enumerate 重複排除

- Priority: Low
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/refactor-device-enumerate-duplication
- Polished: {YYYY-MM-DD}

## 目的

`src/device.rs` の enumerate 系メソッドの著しい重複コードを排除する。

## 優先度根拠

機能的な問題はないが、12 メソッドが実質的に同一パターンを繰り返しており、保守コストが高い。

## 現状

src/device.rs:107-260:

FFI バックエンド 3 種 (coreaudio/pulse/pipewire) × フィルタ 3 種 (None/Input/Output) = 9 メソッド + WASAPI 3 メソッド = 合計 12 メソッドが、`FfiDeviceListImpl::enumerate_XXX(filter)` の呼び出しと `AudioDevice(AudioDeviceInner::Ffi(*d))` への変換・`AudioDeviceListInner::Ffi` の構築という同一パターンを繰り返している。

```rust
pub fn enumerate_coreaudio() -> Result<Self> {
    let inner = FfiDeviceListImpl::enumerate_coreaudio(None)?;
    let devices = inner.as_slice().iter()
        .map(|d| AudioDevice(AudioDeviceInner::Ffi(*d))).collect();
    Ok(Self(AudioDeviceListInner::Ffi { _inner: inner, devices }))
}
// 以下 11 メソッドが同一パターン
```

## 完了条件

- 重複コードが排除される
- 公開 API のシグネチャ・挙動が変わらない
- `cargo clippy` / `cargo test` が通る

## 解決方法

プライベートヘルパ `fn from_ffi(inner: FfiDeviceListImpl) -> Self` と `fn from_wasapi(inner: WasapiDeviceListImpl) -> Self` を導入し、各メソッドを 1 行に削減する。
