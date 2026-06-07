# Error 型の PBT テストを追加する

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

add

## 概要

`src/error.rs` の `Error` 列挙型と `Display` 実装に対する PBT テストが存在しない。`Error` 型は純粋な Rust コードであり、FFI に依存せず PBT でテスト可能である。

## 根拠

CLAUDE.md のテスト規約:
- 「PBT(Property-Based Testing) や Fuzzing でテストを行うこと」
- 「PBT: 型情報（Strategy）に基づいて入力を生成し、プロパティを検証する」

`Error` 型の各バリアントの `Display` 文字列出力は、テストすべきプロパティである。特に `Error::NullPointer` はフィールド値が出力に反映されることを検証する必要がある。

## テスト対象

- `Error::DeviceNotFound.to_string()` → `"audio device not found"`
- `Error::DeviceAccessDenied.to_string()` → `"audio access denied"`
- `Error::SessionCreateFailed.to_string()` → `"failed to create audio session"`
- `Error::SessionStartFailed.to_string()` → `"failed to start audio session"`
- `Error::NullPointer("name").to_string()` → `"null pointer: name"`（パラメータが出力に反映されること）
- `Error::InvalidChannels.to_string()` → `"invalid channels: must be greater than 0"`
- `std::error::Error` trait 実装: `Error::source()` が `None` を返すこと

## 対応方針

### 1. `pbt/tests/prop_error.rs` を新設する

```rust
use proptest::prelude::*;
use shiguredo_audio_device::Error;

fn arb_error() -> impl Strategy<Value = Error> {
    prop_oneof![
        Just(Error::DeviceNotFound),
        Just(Error::DeviceAccessDenied),
        Just(Error::SessionCreateFailed),
        Just(Error::SessionStartFailed),
        Just(Error::InvalidChannels),
        ".*".prop_map(|s| Error::NullPointer(Box::leak(s.into_boxed_str()))),
    ]
}

proptest! {
    /// Display 出力が空でない
    #[test]
    fn display_is_non_empty(err in arb_error()) {
        let msg = err.to_string();
        prop_assert!(!msg.is_empty());
    }

    /// NullPointer の Display 出力にパラメータが含まれる
    #[test]
    fn null_pointer_contains_value(name in ".*") {
        let err = Error::NullPointer(Box::leak(name.into_boxed_str()));
        let msg = err.to_string();
        prop_assert!(msg.contains("null pointer:"));
    }

    /// Error::source() は常に None
    #[test]
    fn source_is_none(err in arb_error()) {
        use std::error::Error as StdError;
        prop_assert!(err.source().is_none());
    }
}
```

### 2. 注意点

- `#[cfg(target_os = "windows")]` でガードされた `ComInitFailed` バリアントは非 Windows 環境では存在しない。strategy では含めず、必要に応じて `#[cfg(target_os = "windows")]` で分岐する
- `NullPointer` のフィールドは `&'static str` であり、strategy で動的に生成した文字列は `Box::leak` で `'static` 化する必要がある。あるいは `NullPointer` 専用の単体テストを別途 `tests/test_error.rs` に配置する

## CHANGES.md への追記

`## develop` セクションの `### misc` サブセクションに以下のエントリを追記する:

```
- [ADD] Error 型の PBT テストを追加する
  - @ユーザー名
```
