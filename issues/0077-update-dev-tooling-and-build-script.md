# 開発ツール設定と build.rs のメッセージを整備する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-dev-tooling-and-build-script
- Polished: {YYYY-MM-DD}

## 目的

Makefile / prek / build.rs の設定とメッセージを整え、開発環境と CI の挙動を揃える。

## 現状

- `Makefile` の `fuzzing` / `fuzzing-parallel` は `nproc` を使っており、macOS には `nproc` が無いため `make fuzzing` が失敗する。
- `Makefile` の `clippy` ターゲットは `--all-targets` が無く、prek の `cargo clippy --workspace --all-targets` と対象が異なる。
- `Makefile` の `fuzzing-parallel` は `fuzz/logs` を生成するが、`.gitignore` に対象が無く、実行前にログを消去していない。`fuzz: fuzzing` は実体の無い別名。
- `prek.toml` の先頭 2 行が英語コメントで `AGENTS.md` の「コメントは全て日本語」に反する。C / Objective-C の formatter と Python の ruff / ty フックが無い。`.markdownlint.jsonc` はどのフックからも参照されていない。
- `build.rs` はサポート外の `target_os` で `panic!` するため、iOS / Android / BSD などでは原因が分かりにくいビルド失敗になる。bindgen の失敗メッセージもどのヘッダ / バックエンドで失敗したか、libclang が見つからないのかを特定できない。
- `build.rs` は同じ `CARGO_FEATURE_*` の判定と `match target_os` を 2 回書いており、バックエンド追加時に 3 箇所 (check-cfg、1 回目、2 回目) を同期する必要がある。片方だけ直す事故が起きやすい。

## 設計方針

- Makefile の `nproc` を `nproc 2>/dev/null || sysctl -n hw.ncpu` のようなフォールバックに変える。`clippy` に `--all-targets` を付ける。`/fuzz/logs` を `.gitignore` に追加し、実行前に削除する。`fuzz` の別名を整理する。
- `prek.toml` の英語コメントを日本語にするか削除する。`.clang-format` と clang-format フックを追加する。Python の ruff / ty フックを canary.py に限定して追加する。markdownlint を使うならフックを追加し、使わないなら設定ファイルを削除する。
- `build.rs` のサポート外 OS のメッセージを、対応プラットフォームを明示した内容にする。bindgen の失敗メッセージにバックエンド名と libclang の確認を含める。

## 完了条件

- `make clippy` / `make fuzzing` が macOS で動作する。
- `fuzz/logs` が git 管理対象外になり、古いログが混入しない。
- prek のコメント言語・formatter・フックの有無が方針どおりになる。
- サポート外ターゲットと bindgen 失敗時のメッセージで原因が分かる。
- `cargo fmt` / `cargo clippy --workspace --all-targets` が通る。
