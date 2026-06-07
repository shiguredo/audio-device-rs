# examples のエラーメッセージを英語に修正する

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## 概要

examples のエラーメッセージが日本語で書かれており、CLAUDE.md の「エラーメッセージは全て英語」に違反している。

## 対象箇所

- `examples/device_info.rs:9` — `eprintln!("デバイスの列挙に失敗しました: {e}");`
- `examples/device_list.rs:7` — `eprintln!("デバイスの列挙に失敗しました: {e}");`

## 根拠

CLAUDE.md 規約:
> エラーメッセージは全て英語

stderr への出力はエラーメッセージに該当するため、英語で出力する必要がある。

また CLAUDE.md のサンプルに関する規定:
> サンプルは **お手本** なので性能と堅牢性を両立させること

例示コードとしてのお手本としても、規約を遵守すべきである。

## 対応方針

両ファイルの `eprintln!("デバイスの列挙に失敗しました: {e}")` を `eprintln!("Failed to enumerate devices: {e}")` に変更する。

## CHANGES.md への追記

`## develop` セクションの `### misc` サブセクションに以下のエントリを追記する:

```
- [FIX] examples のエラーメッセージを日本語から英語に修正する
  - @ユーザー名
```
