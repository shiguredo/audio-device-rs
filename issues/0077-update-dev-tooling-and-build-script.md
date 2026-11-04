# 開発ツール設定と build.rs のメッセージを整備する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-dev-tooling-and-build-script
- Polished: 2026-11-04

## 目的

Makefile / prek / build.rs の設定とメッセージを整え、開発環境の挙動を prek と揃える。あわせて、CI の clippy 対象も Makefile / prek と揃える。

## 現状

- `Makefile` の `fuzzing` / `fuzzing-parallel` は `nproc` を使っており、標準の macOS には `nproc` が無い。`make fuzzing` は `nproc` が見つからない旨のエラーを表示したうえで `FORKS=1` にフォールバックして処理を続けるため失敗はしないが、コア数に応じた fork 数を使えず、`echo "Using fork=$$FORKS on $$(nproc) cores"` のコア数も空になる。
- `Makefile` の `fuzzing-parallel` は `FORKS` を計算して表示するが、実際の `cargo fuzz run` には `-fork=1` を渡しており、`FORKS` を使っていない。並列化は `-P <ターゲット数>` で行っている。コメントの「fork 数はコア数に応じて自動調整」も実態と一致しない。
- `Makefile` の `clippy` ターゲットは `cargo clippy --workspace -- -D warnings` で、prek の `cargo clippy --workspace --all-targets -- -D warnings` と対象が異なる。CI の clippy も `cargo clippy --workspace ... -- -D warnings` で `--all-targets` が無い。
- `Makefile` の `fuzzing-parallel` は `fuzz/logs` を生成するが、`.gitignore` に対象が無く、実行前にログを消去していない。`fuzz: fuzzing` は `fuzzing` を呼ぶだけの別名である。
- `prek.toml` の先頭 2 行が英語コメントで `AGENTS.md` の「コメントは全て日本語にすること」に反する。C / Objective-C の formatter と Python の ruff / ty フックが無い。`.markdownlint.jsonc` はどのフックからも参照されていない。
- `build.rs` はサポート外の `target_os` で `panic!` する。メッセージは対象 OS 名だけを示し、対応プラットフォームを示さない。bindgen の `Err` に対するメッセージは `Failed to generate bindings` で、エラー内容は付くがどのバックエンドで失敗したかは示さない。libclang が見つからない場合は bindgen が `Unable to find libclang` で panic するため、これは特定できる。
- `build.rs` は同じ `CARGO_FEATURE_*` の判定と `match target_os` を 2 回書いており、バックエンド追加時に 3 箇所 (check-cfg、1 回目、2 回目) を同期する必要がある。2 回目の `match` の `_` 分岐は 1 回目で panic 済みのため到達しない。片方だけ直す事故が起きやすい。

## 設計方針

- Makefile の `fuzzing` でコア数を 1 回取得し、`nproc 2>/dev/null || sysctl -n hw.ncpu` のフォールバックを使う。取得できない場合は 1 にする。`FORKS` と `echo` の両方で同じコア数を使う。`fuzzing-parallel` の未使用の `FORKS` 計算と `echo` を削除し、コメントを実態 (ターゲットごとに `-fork=1`、ターゲット数を `-P` の並列度にする) に合わせる。
- `Makefile` の `clippy` に `--all-targets` を付け、CI の clippy にも `--all-targets` を付けて、prek の `cargo clippy --workspace --all-targets -- -D warnings` と対象を揃える。
- `/fuzz/logs` を `.gitignore` に追加し、`fuzzing-parallel` の実行前に `fuzz/logs` を削除する。`fuzz: fuzzing` を削除し、`.PHONY` から `fuzz` を外す。
- `prek.toml` の先頭 2 行の英語コメントを日本語に置き換える。`#:schema` 行はそのまま残す。
- `prek run --all-files` の既存の指摘として、`CODEBASE.md` の末尾に改行を追加する。
- `.clang-format` を追加し、既存の C / Objective-C コードのスタイル (4 スペースインデント、ポインタは型側) に合わせる。既存の C / Objective-C / ヘッダを `.clang-format` で整形する。prek には clang-format の外部フック (例: `https://github.com/pre-commit/mirrors-clang-format`) を追加し、C / C++ / Objective-C を対象にする。
- prek には ruff の check / format と ty の check の外部フック (例: `https://github.com/astral-sh/ruff-pre-commit` / `https://github.com/astral-sh/ty-pre-commit`) を追加し、`canary.py` に限定する。`canary.py` は ruff の指摘 (UP015 / UP045) と format 差分を解消し、ruff / ty が通る状態にする。
- markdownlint は使わない。`.markdownlint.jsonc` を削除する。prek のフックも CI も markdownlint を参照しておらず、現行の設定で既存の `.md` を検査すると既存ファイルが多数失敗するためである。
- `build.rs` は `target_os` と feature からバックエンドの有効状態を 1 つの構造体にまとめ、その結果を cfg の出力と C コンパイル / bindgen の両方で使う。2 回目の `match target_os` と到達しない `_` 分岐を削除する。check-cfg の宣言は cfg 名の一覧から出力する。
- サポート外の `target_os` では、対応プラットフォーム (`macos` / `linux` / `windows`) を明示したメッセージで panic する。bindgen の `Err` にはバックエンド名を含め、libclang の確認を促すメッセージにする。

## 完了条件

- macOS で `make clippy` / `make fuzzing` がエラーなく動作し、`make fuzzing` がコア数に応じた `FORKS` を使う。
- `fuzz/logs` が git 管理対象外になり、`make fuzzing-parallel` の実行前に古いログが削除される。
- `Makefile` の `clippy` と CI の clippy が `--all-targets` で実行される。
- `prek.toml` のコメントが日本語になり、`prek validate-config` が通る。
- `.clang-format` が追加され、C / Objective-C / ヘッダが整形されている。clang-format フックが C / C++ / Objective-C を対象にする。
- ruff の check / format と ty の check のフックが `canary.py` に限定して追加され、`canary.py` が ruff / ty を通る。
- `.markdownlint.jsonc` が削除されている。
- `prek run --all-files` が通る。
- サポート外の `target_os` では、対応プラットフォームを明示したメッセージで panic する。
- bindgen の `Err` では、バックエンド名を含むメッセージで panic する。
- `build.rs` のバックエンド判定が 1 箇所にまとまり、2 回目の `match target_os` が無い。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` が通る。
