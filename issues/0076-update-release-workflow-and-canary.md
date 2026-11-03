# リリースワークフローと canary スクリプトを堅牢化する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-release-workflow-and-canary
- Polished: 2026-11-04

## 目的

タグ push から crates.io 公開までの手順で、バージョン不一致・テスト未実行・ビルド依存不足による事故を防ぐ。

## 現状

- `canary.py` はコミット直後にタグを作成して push する。`release.yml` はタグ push をトリガーに GitHub Release を作成して `cargo publish` するが、テストを実行しない。テストが壊れた canary でもコンパイルが通れば crates.io に公開される (crates.io のバージョンは削除できず yank のみ)。
- `Cargo.toml` のバージョンとタグの一致を検証せずに `cargo publish` する。`cargo package --list` / `cargo publish --dry-run` によるパッケージ内容の検証も無い。
- `release.yml` の publish ジョブは `libpulse-dev` しかインストールせず、`rustup show` も実行していない。CI は `libclang-dev` を明示的にインストールしており、ビルド依存が揃っていない。`build.rs` は `bindgen` を使うため、Linux では `libclang` が必要である。
- `release.yml` の `github-release` / `publish` ジョブに `timeout-minutes` が無い。GitHub Release の本文は "Release <version>" だけで `CHANGES.md` を反映していない。
- `CHANGES.md` には `## develop` とリリース済みバージョンのセクションがあり、canary バージョン (`2026.4.0-canary.3` など) に対応するセクションは無い。canary の GitHub Release 本文に使うセクションが決まっていない。
- `canary.py` は `Cargo.toml` を書き換えた後に `cargo update` を `check=True` で実行するため、`cargo update` が失敗するとバージョンだけ上がった未コミット状態が残り、再実行で canary 番号がさらに進む。`--dry-run` でも確認プロンプトが出る。エラーメッセージの末尾にピリオドがある。
- `canary.py` は `Optional[str]` を使っている。

## 設計方針

- タグ名と `Cargo.toml` の `version` の一致を、GitHub Release 作成と `cargo publish` の
  両方より前に検証する。検証は専用の `preflight` ジョブで行い、`github-release` と
  `publish` は `preflight` 成功後に実行する。不一致の場合は GitHub Release を作成せず、
  `cargo publish` もしない。タグ名は既存の canary.py と同じく `v` を付けない
  `<version>` 形式とする。
- `preflight` と `publish` で、CI の pulse ジョブと同じビルド依存 (`libclang-dev`、`libpulse-dev`) をインストールし、`rustup show` を実行する。`preflight` は `publish` と同じ `ubuntu-24.04` で実行する。
- `cargo package --list` と `cargo publish --dry-run` を CI または `release.yml` の
  ジョブに追加する。README の手動手順だけにしない。`cargo publish --dry-run` は実際の
  publish と同じビルド依存を必要とするため、依存をインストールした後に実行する。
  `cargo package --list` の出力を検査し、`build.rs` と `src/` 配下のファイルが含まれる
  ことを確認する。
- `preflight` / `github-release` / `publish` ジョブに `timeout-minutes` を設定する。
- GitHub Release の本文は `CHANGES.md` から生成する。canary タグ (`<version>-canary.<n>`) は `## develop` セクション、stable タグ (`<version>`) は `## <version>` セクションを使う。該当セクションが無い場合は Release 作成を失敗させる。
- `canary.py` は `Cargo.toml` を書き換えた後に `cargo update` を実行する。`cargo update`
  が失敗した場合は `Cargo.toml` と `Cargo.lock` を元に戻す。`git commit` が失敗した
  場合も同様に両ファイルを元に戻し、`git add` を取り消す。dry-run では確認プロンプトを
  出さない。エラーメッセージのピリオドを削除し、`Optional[str]` を `str | None` に
  統一する。
- `canary.py` はコミットを push した後、そのコミットの CI が成功するまで待ってから
  タグを作成して push する。CI の待機には GitHub CLI (`gh`) を使い、CI の実行が
  見つからない場合や `gh` の呼び出しが失敗した場合もタグを作成しない。CI が失敗した
  場合はタグを作成しない。stable リリースも README または docs のリリース手順で、
  CI 成功後にタグを push する。
- リリース手順を README または docs に追加する。手順には、canary と stable のタグ形式、
  `gh` の認証、`CHANGES.md` の更新 (stable リリース時は `## develop` を `## <version>`
  に変更し、`**リリース日**: YYYY-MM-DD` を記載する)、CI 成功の確認、`canary.py` の
  実行、失敗時の復旧を含める。

## 完了条件

- バージョン不一致のタグでは GitHub Release が作成されず、`cargo publish` も実行されない。
- `release.yml` の `preflight` / `publish` ジョブのビルド依存が CI の pulse ジョブと一致している (`libclang-dev` のインストールと `rustup show` を含む)。
- `cargo package --list` と `cargo publish --dry-run` が CI または `release.yml` で自動実行され、パッケージ内容の検証が自動化されている。
- `release.yml` の `preflight` / `github-release` / `publish` ジョブに `timeout-minutes` が設定されている。
- GitHub Release の本文が `CHANGES.md` の該当セクション (canary は `## develop`、stable は `## <version>`) から生成され、セクションが無い場合は失敗する。
- CI が失敗したコミットにはタグが付かず、`cargo publish` されない。
- canary.py の `cargo update` または `git commit` が失敗した場合に `Cargo.toml` / `Cargo.lock` と git index が元の状態に戻り、作業ツリーが半端な状態で残らない。
- canary.py の dry-run で確認プロンプトが出ず、エラーメッセージの末尾にピリオドが無く、`Optional[str]` が `str | None` に統一されている。
- リリース手順が README または docs に記載され、canary と stable のタグ形式、`gh` の認証、`CHANGES.md` の更新、CI 成功の確認、`canary.py` の実行、失敗時の復旧が含まれている。
