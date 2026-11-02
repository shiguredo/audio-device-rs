# リリースワークフローと canary スクリプトを堅牢化する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-release-workflow-and-canary
- Polished: {YYYY-MM-DD}

## 目的

タグ push から crates.io 公開までの手順で、バージョン不一致・テスト未実行・ビルド依存不足による事故を防ぐ。

## 現状

- `canary.py` はコミット直後にタグを作成して push する。`release.yml` はタグ push をトリガーに GitHub Release を作成して `cargo publish` するが、テストを実行しない。テストが壊れた canary でもコンパイルが通れば crates.io に公開される (crates.io のバージョンは削除できず yank のみ)。
- `Cargo.toml` のバージョンとタグの一致を検証せずに `cargo publish` する。`cargo package --list` / `cargo publish --dry-run` によるパッケージ内容の検証も無い。
- `release.yml` の publish ジョブは `libpulse-dev` しかインストールせず、`rustup show` も実行していない。CI は `libclang-dev` を明示的にインストールしており、ビルド依存が揃っていない。
- `release.yml` の `github-release` / `publish` ジョブに `timeout-minutes` が無い。GitHub Release の本文は "Release <version>" だけで `CHANGES.md` を反映していない。
- `canary.py` は `Cargo.toml` を書き換えた後に `cargo update` を `check=True` で実行するため、`cargo update` が失敗するとバージョンだけ上がった未コミット状態が残り、再実行で canary 番号がさらに進む。`--dry-run` でも確認プロンプトが出る。エラーメッセージの末尾にピリオドがある。

## 設計方針

- タグ名と `Cargo.toml` の `version` の一致を publish 前に検証する。
- CI と release.yml でビルド依存 (`libclang-dev`、`rustup show`) を揃える。
- `cargo package --list` と `cargo publish --dry-run` を CI またはリリース手順に追加する。
- ジョブに `timeout-minutes` を設定し、GitHub Release の本文を `CHANGES.md` の該当セクションから生成する。
- canary.py は `cargo update` を先に実行するか、失敗時に `Cargo.toml` を元に戻す。dry-run では確認プロンプトを出さない。エラーメッセージのピリオドを削除し、`Optional[str]` を `str | None` に統一する。
- タグ push 前に CI の成功を待つなど、公開前の検証手順を定義する。

## 完了条件

- バージョン不一致のタグでは publish されない。
- publish のビルド依存が CI と一致している。
- パッケージ内容の検証が自動化されている。
- canary.py の失敗時に作業ツリーが半端な状態で残らない。
- リリース手順が README か docs に記載されている。
