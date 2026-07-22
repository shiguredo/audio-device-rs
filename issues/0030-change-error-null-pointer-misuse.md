# Error::NullPointer の意味的誤用修正

- Priority: Low
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/change-error-null-pointer-misuse
- Polished: {YYYY-MM-DD}

## 目的

`src/capture_ffi.rs` と `src/playback_ffi.rs` で、`CString::new` 失敗時（文字列内部に NUL バイトを含む場合）に `Error::NullPointer` を返している意味的誤用を修正する。

## 優先度根拠

機能的な問題はないが、エラーハンドリング側で `NullPointer` を「C 側から null ポインタが返された」と解釈すると誤った診断につながる。

## 現状

src/capture_ffi.rs:79-82、src/playback_ffi.rs:89-92:

```rust
Some(Err(_)) => {
    return Err(Error::NullPointer("device_id contains null byte"));
}
```

`NullPointer` は「ポインタが null である」ことを示すバリアント名であり、「文字列に内部 NUL バイトが含まれる」という全く異なるエラー条件に流用されている。

## 完了条件

- NUL バイト含む device_id に対して意味的に正しいエラーバリアントが返される
- `cargo clippy` / `cargo test` が通る

## 解決方法

`Error::InvalidDeviceId` 等の専用バリアントを追加する。
