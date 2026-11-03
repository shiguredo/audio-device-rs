# docs.rs 向けのビルド設定を追加する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-docs-rs-build-metadata
- Polished: 2026-11-04

## 目的

docs.rs の既定ビルドが macOS ターゲットのクロスコンパイルで失敗しないようにする。

## 現状

- `Cargo.toml` に `[package.metadata.docs.rs]` が無いため、docs.rs は既定で `x86_64-unknown-linux-gnu` / `aarch64-apple-darwin` / `x86_64-pc-windows-msvc` / `aarch64-unknown-linux-gnu` / `i686-pc-windows-msvc` の 5 ターゲットをビルドする。
- `build.rs` の `build_coreaudio` は `audio_coreaudio.m` を `cc` でコンパイルし、AudioToolbox / CoreAudio / Foundation をリンクする。`aarch64-apple-darwin` は Linux ホストからのクロスビルドになり、Apple SDK が無いため失敗する。README の docs.rs バッジが機能しない恐れがある。
- `x86_64-unknown-linux-gnu` は docs.rs のビルド環境に `libclang-dev` と `libpulse-dev` が含まれるため成功する見込みだが、`DOCS_RS` を考慮した分岐は無い。

## 設計方針

- `[package.metadata.docs.rs]` に次の設定を追加し、既定ターゲットだけをビルドする。

  ```toml
  [package.metadata.docs.rs]
  default-target = "x86_64-unknown-linux-gnu"
  targets = []
  ```

- `all-features` は指定せず、デフォルト feature のみをビルドする。docs.rs のビルド環境 (crates-build-env) には `libpipewire-0.3-dev` が含まれないため、`pipewire` を有効にしない。
- `build.rs` に `DOCS_RS` 分岐は追加しない。docs.rs のビルド環境 (crates-build-env) には `libclang-dev` と `libpulse-dev` が含まれ、`x86_64-unknown-linux-gnu` のみをビルドするため、C コンパイルと bindgen はそのまま実行できる。docs.rs のビルドが失敗した場合にのみ、`DOCS_RS` を見て C コンパイルと bindgen を省略する分岐を検討する。追加する場合はバックエンドの cfg と矛盾しないようにする。
- 本 issue では `Cargo.toml` の `include` を変更しない。`cargo package --list` で `build.rs` と `src/` 配下のファイルが含まれることを確認する。README の相対リンクと `docs/` の同梱は本 issue の対象外とする。
- リリース後に docs.rs のビルド結果と README の docs.rs バッジを確認する。

## 完了条件

- `Cargo.toml` に `[package.metadata.docs.rs]` が追加され、`default-target = "x86_64-unknown-linux-gnu"` と `targets = []` が設定され、`all-features` は指定されていない。
- `cargo package --list` に `build.rs` と `src/` 配下のファイルが含まれ、docs.rs のビルドに必要なファイルが欠けていない。
- リリース後に docs.rs のビルドが成功し、README の docs.rs バッジが有効になることを確認する。
