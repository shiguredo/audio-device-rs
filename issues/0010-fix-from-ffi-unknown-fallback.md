# from_ffi() の未知の値に対する無条件フォールバックを修正する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

fix

## 概要

`AudioDeviceType::from_ffi()` と `AudioFormat::from_ffi()` が、未知の FFI 定数値に対して無条件に `Input` / `S16` を返している。将来 C 側で新しいデバイス種別やフォーマットが追加された場合、Rust 側が更新されなくてもコンパイルが通り、誤った種別・フォーマットで動作し続ける。

## 対象箇所

- `src/device.rs:17-22` — `AudioDeviceType::from_ffi()`
- `src/device.rs:35-40` — `AudioFormat::from_ffi()`

```rust
fn from_ffi(device_type: i32) -> Self {
    match device_type {
        x if x == ffi::AUDIO_DEVICE_TYPE_OUTPUT as i32 => AudioDeviceType::Output,
        _ => AudioDeviceType::Input,  // ← 未知の値が Input に丸め込まれる
    }
}
```

## 根拠

フォーマットを誤認識するとオーディオデータの解釈が破綻し、ノイズや無音が発生する。将来的な拡張時に気付かれずに誤動作し続ける「沈黙の誤動作」を引き起こす。

## 対応方針

`from_ffi()` の戻り値を `Result<Self>` に変更し、未知の値に対しては `Err` を返す。`AudioDevice` の構築時 (`device.rs:120-121`) および `frame_callback` 内 (`capture.rs:266`) の呼び出し側も併せて修正する。
