# Error 型の PBT テストを追加する

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

add

## 概要

`src/error.rs` の `Error` 列挙型と `Display` 実装に対する PBT テストが存在しない。`Error` 型は純粋な Rust コードであり、FFI に依存せず PBT でテスト可能である。

## 根拠

CLAUDE.md のテスト規約:
- 「PBT(Property-Based Testing) や Fuzzing でテストを行うこと」
- 「PBT: 型情報（Strategy）に基づいて入力を生成し、プロパティを検証する」

`Error` 型の各バリアントの `Display` 文字列出力は、テストすべきプロパティである。特に `Error::NullPointer` はフィールド値が出力に反映されることを検証する必要がある。

## 対象の未テスト項目

- `Error::DeviceNotFound.to_string()` → `"audio device not found"`
- `Error::DeviceAccessDenied.to_string()` → `"audio access denied"`
- `Error::SessionCreateFailed.to_string()` → `"failed to create audio session"`
- `Error::SessionStartFailed.to_string()` → `"failed to start audio session"`
- `Error::NullPointer("device name").to_string()` → `"null pointer: device name"`
- `Error::InvalidChannels.to_string()` → `"invalid channels: must be greater than 0"`
- `Result<T>` 型エイリアスの `?` 演算子によるエラー伝播
- `std::error::Error` trait 実装 (`Error::source()` が `None` を返すこと)

## 対応方針

1. `pbt/tests/prop_error.rs` を作成する
2. `Error` バリアントの `Display` 出力を検証する PBT を実装する
3. `std::error::Error` trait の基本的な挙動（`Error::source()` が `None` を返すこと）を検証する

## 注意点

`#[cfg(target_os = "windows")]` でガードされた `ComInitFailed` バリアントは、非 Windows 環境では存在しないため、strategy で分岐させる必要がある。
