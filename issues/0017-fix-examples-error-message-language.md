# examples のエラーメッセージを英語に修正する

Created: 2026-06-07
Model: deepseek-v4-pro

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

stderr への出力はエラーメッセージに該当するため、英語で出力する必要がある。例示コードとしての **お手本** としても、規約を遵守すべきである（CLAUDE.md「サンプルは **お手本** なので性能と堅牢性を両立させること」）。

## 対応方針

`eprintln!("Failed to enumerate devices: {e}")` に変更する。
