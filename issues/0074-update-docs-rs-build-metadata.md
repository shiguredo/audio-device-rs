# docs.rs 向けのビルド設定を追加する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-docs-rs-build-metadata
- Polished: {YYYY-MM-DD}

## 目的

docs.rs の既定ビルドが macOS ターゲットのクロスコンパイルで失敗しないようにする。

## 現状

- `Cargo.toml` に `[package.metadata.docs.rs]` が無いため、docs.rs は既定で `x86_64-unknown-linux-gnu` / `aarch64-apple-darwin` / `x86_64-pc-windows-msvc` / `aarch64-unknown-linux-gnu` / `i686-pc-windows-msvc` の 5 ターゲットをビルドする。
- `build.rs` の `build_coreaudio` は `audio_coreaudio.m` を `cc` でコンパイルし、AudioToolbox / CoreAudio / Foundation をリンクする。`aarch64-apple-darwin` は Linux ホストからのクロスビルドになり、Apple SDK が無いため失敗する。README の docs.rs バッジが機能しない恐れがある。
- Linux ターゲットは docs.rs のビルド環境に `libclang-dev` と `libpulse-dev` が含まれるため成功する見込みだが、`DOCS_RS` を考慮した分岐は無い。

## 設計方針

- `[package.metadata.docs.rs]` に `default-target = "x86_64-unknown-linux-gnu"` と `targets = []` を指定し、既定ターゲットだけをビルドする。
- 必要であれば `DOCS_RS` 環境変数を見て、build.rs の C コンパイルと bindgen を省略する分岐を検討する。ただしバックエンドの cfg と矛盾しないようにする。
- 公開後に docs.rs のビルド結果を確認する。

## 完了条件

- docs.rs のビルドが成功する。
- README の docs.rs バッジが有効になる。
- `cargo package --list` の内容が意図どおりである。
