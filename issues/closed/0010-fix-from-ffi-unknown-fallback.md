# from_ffi() の未知の値に対する無条件フォールバックを修正する

Created: 2026-06-07
Completed: 2026-06-08
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## プラットフォームスコープ

この issue の対象は `src/device.rs`（macOS/Linux）のみ。`src/device_windows.rs` (Windows) には `from_ffi()` が存在しないため対象外。

## 概要

`AudioDeviceType::from_ffi()` と `AudioFormat::from_ffi()` が、未知の FFI 定数値に対して無条件に `Input` / `S16` を返している。将来 C 側で新しいデバイス種別やフォーマットが追加された場合、Rust 側が更新されなくてもコンパイルが通り、誤った種別・フォーマットで動作し続ける。

## 再現手順

1. 現在の C ライブラリが未知のデバイス種別 (`ffi::AUDIO_DEVICE_TYPE_OUTPUT` でも `ffi::AUDIO_DEVICE_TYPE_INPUT` でもない値) を `ffi::audio_device_type()` 経由で返した場合を想定する
2. `AudioDeviceType::from_ffi()` は該当値を `Input` に丸め込むため、デバイス列挙結果に誤った種別で表示される
3. 同様に `AudioFormat::from_ffi()` は未知のフォーマット値を `S16` に丸め込むため、キャプチャされたオーディオデータが誤ったフォーマットとして解釈されノイズになる

## 対象箇所

- `src/device.rs:16-22` — `AudioDeviceType::from_ffi()` (private)
- `src/device.rs:34-40` — `AudioFormat::from_ffi()` (pub(crate))
- `src/device.rs:120-124` — `from_ffi` 呼び出し元（`enumerate_internal` 内のクロージャ）
- `src/capture.rs:266` — `from_ffi` 呼び出し元（`frame_callback` extern "C" 関数内）
- `src/error.rs` — Error バリアント追加

## 根拠

フォーマットを誤認識するとオーディオデータの解釈が破綻し、ノイズや無音が発生する。将来の拡張時に気付かれずに誤動作し続ける「沈黙の誤動作」を引き起こす。

## 対応方針

### 1. Error バリアントの追加

`src/error.rs` に以下を追加する:

```rust
pub enum Error {
    // ... 既存バリアント ...
    UnknownFormat(i32),
    UnknownDeviceType(i32),
}
```

`Display` 実装:

```rust
Error::UnknownFormat(v) => write!(f, "unknown audio format: {}", v),
Error::UnknownDeviceType(v) => write!(f, "unknown audio device type: {}", v),
```

### 2. from_ffi() の戻り型変更

**`AudioDeviceType::from_ffi()` (device.rs:16-22)**:

```rust
// 変更後
fn from_ffi(device_type: i32) -> Result<Self> {
    match device_type {
        x if x == ffi::AUDIO_DEVICE_TYPE_OUTPUT as i32 => Ok(AudioDeviceType::Output),
        x if x == ffi::AUDIO_DEVICE_TYPE_INPUT as i32 => Ok(AudioDeviceType::Input),
        other => Err(Error::UnknownDeviceType(other)),
    }
}
```

**`AudioFormat::from_ffi()` (device.rs:34-40)**:

```rust
// 変更後
pub(crate) fn from_ffi(format: i32) -> Result<Self> {
    match format {
        x if x == ffi::AUDIO_FORMAT_S16 as i32 => Ok(AudioFormat::S16),
        x if x == ffi::AUDIO_FORMAT_F32 as i32 => Ok(AudioFormat::F32),
        other => Err(Error::UnknownFormat(other)),
    }
}
```

### 3. 呼び出し側の修正

#### `device.rs:120-124` — enumerate_internal 内

現在 `filter_map` と `NonNull::new().map()` のチェーンで `AudioDevice` を構築している。`from_ffi` が `Result` を返すようになるため、未知のデバイス種別を持つデバイスは `filter_map` によりスキップする。`map` を `and_then` に置き換え、`Err` 時に `None` を返す:

```rust
// 変更後
NonNull::new(device_ptr).and_then(|raw| {
    let device_type = AudioDeviceType::from_ffi(unsafe {
        ffi::audio_device_type(raw.as_ptr())
    })
    .ok()?;
    Some(AudioDevice { raw, device_type })
})
```

#### `capture.rs:266` — frame_callback 内

`extern "C"` コールバック内で `from_ffi` が `Err` を返した場合、未知のフォーマットのデータは安全に処理できないため早期 return する。コールバックの戻り値は `()` でありエラー伝播はできないが、`log` crate は依存ゼロ方針により使用しないためサイレントにスキップする:

```rust
// 変更後
let audio_format = match AudioFormat::from_ffi(format) {
    Ok(f) => f,
    Err(_) => return,
};
```

### 4. 後方互換性

- `AudioDeviceType::from_ffi()` は private のため、外部への影響はない
- `AudioFormat::from_ffi()` は `pub(crate)` であり、呼び出し元の修正が必要だがクレート内部に閉じる
- 外部 API に変更はないため、ユーザーへの破壊的変更はない

### 5. CHANGES.md への追記

`## develop` セクションに以下のエントリを追記する:

```
- [FIX] from_ffi() が未知の FFI 定数値を無条件に Input/S16 へ丸め込む問題を修正する
  - @ユーザー名
```

### 6. テスト戦略

- `from_ffi` の変更は PBT でテスト可能だが、入力空間が小さい（既知の定数値 + 任意の i32）ため **単体テスト** として `src/device.rs` の `#[cfg(test)] mod tests` に追加する
- テストケース:
  - 既知の定数値が正しいバリアントにマッピングされること
  - 未知の値が `Err` を返すこと（複数の任意値で確認）
- `AudioFormat::from_ffi` は `pub(crate)` のため、fuzzing ターゲットとして `cargo-fuzz` に追加し、任意の `i32` 入力でパニックしないことを検証する

## 解決方法

- `src/error.rs` に `UnknownFormat(i32)` と `UnknownDeviceType(i32)` の Error バリアントを追加した
- `AudioDeviceType::from_ffi()` の戻り型を `Self` から `Result<Self>` に変更し、未知の値に対して `Err(Error::UnknownDeviceType(other))` を返すようにした
- `AudioFormat::from_ffi()` の戻り型を `Self` から `Result<Self>` に変更し、未知の値に対して `Err(Error::UnknownFormat(other))` を返すようにした
- `enumerate_internal()` 内の `map` を `and_then` に置き換え、`from_ffi` が `Err` を返したデバイスをスキップするようにした
- `capture.rs` の `frame_callback` 内で `from_ffi` が `Err` を返した場合は早期 return して処理を打ち切るようにした
- 既知の定数値と未知の値に対する単体テストを `src/device.rs` の `#[cfg(test)] mod tests` に追加した
- `CHANGES.md` に [FIX] エントリを追記した
