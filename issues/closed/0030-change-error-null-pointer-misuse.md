# Error::NullPointer の意味的誤用修正

- Priority: Low
- Created: 2026-07-20
- Completed: 2026-10-23
- Model: Qwen 3
- Branch: feature/change-error-null-pointer-misuse
- Polished: {YYYY-MM-DD}

## 目的

`src/capture_ffi.rs` と `src/playback_ffi.rs` で、`CString::new` 失敗時（文字列内部に NUL バイトを含む場合）に `Error::NullPointer` を返している意味的誤用を修正する。

## 優先度根拠

機能的な問題はないが、エラーハンドリング側で `NullPointer` を「C 側から null ポインタが返された」と解釈すると誤った診断につながる。

## 現状

`src/capture_ffi.rs` の `FfiCaptureImpl::new` と `src/playback_ffi.rs` の `FfiPlaybackImpl::new` で、`CString::new` の失敗時に次のように `Error::NullPointer` を返していた:

```rust
Some(Err(_)) => {
    return Err(Error::NullPointer("device_id contains null byte"));
}
```

`NullPointer` は「ポインタが null である」ことを示すバリアント名であり、「文字列に内部 NUL バイトが含まれる」という全く異なるエラー条件に流用されている。

## 完了条件

- NUL バイトを含む device_id に対して意味的に正しいエラーバリアントが返される
- `cargo clippy` / `cargo test` が通る

## 解決方法

`Error::InvalidDeviceId` を追加し、`src/capture_ffi.rs` と `src/playback_ffi.rs` で `CString::new` が失敗した場合に `Error::NullPointer` ではなく `Error::InvalidDeviceId` を返すようにした。

- NUL バイトを含むデバイス ID を C 側へ渡す前に拒否する
- `Error` の PBT に `InvalidDeviceId` を追加し、全バリアントの `Display` 出力を検証する
- NUL バイトを含むデバイス ID に対して `AudioCapture` / `AudioPlayback` が `InvalidDeviceId` を返す単体テストを追加する
- `Error::InvalidDeviceId` の `Display` 出力を検証する単体テストを追加する
- `CHANGES.md` の `## develop` に `[CHANGE]` エントリを追記する

`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` が Windows と WSL Ubuntu 24.04 上でパスすることを確認した。Linux では PulseAudio、PipeWire、両方有効、バックエンド無効の各構成を確認した。

## 後方互換

`Error` に `InvalidDeviceId` バリアントを追加するため、`Error` を網羅的にマッチしている利用側ではコンパイルエラーになる。NUL バイトを含むデバイス ID を指定した場合の返り値は `Error::NullPointer` から `Error::InvalidDeviceId` に変わる。
