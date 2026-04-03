# pbt クレートの default-features が pipewire ビルドで feature 競合を起こす

Created: 2026-04-03
Model: Opus 4.6

## 概要

`pbt/Cargo.toml` の `shiguredo_audio_device` 依存に `default-features = false` が指定されていないため、`cargo test --workspace --no-default-features --features pipewire` 実行時に Cargo の feature 統合で `pulse`（default）と `pipewire` が同時に有効になり、`build.rs` の排他チェックで panic する。

## 再現手順

```bash
cargo test --workspace --no-default-features --features pipewire
```

## エラー

```
thread 'main' panicked at build.rs:21:17:
features "pulse" and "pipewire" are mutually exclusive
```

## 根本原因

`pbt/Cargo.toml` で `shiguredo_audio_device = { path = ".." }` と記述しており、default features（`pulse`）が有効になる。ワークスペース全体でビルドすると Cargo が feature を統合するため、ルートクレートに `--features pipewire` を指定しても `pbt` 経由で `pulse` が有効になる。

## 解決方法

`pbt/Cargo.toml` の依存に `default-features = false` を追加した。PBT テストはプラットフォーム固有の feature を使用しないため影響なし。

Completed: 2026-04-03
