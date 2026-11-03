# ドキュメントとコード内のコメントやメッセージの記述を実装に合わせる

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-repository-documentation
- Polished: 2026-11-04

## 目的

README / docs / `CHANGES.md` / コードや設定ファイル内のコメント、`build.rs` の panic メッセージの記述が実装と食い違っている箇所を修正する。

## 現状

- `README.md` の「ランタイム依存クレートなし (システムライブラリのみ)」は、Windows が `windows` クレートに依存している実態と厳密には一致しない。
- `README.md` の Ubuntu 向けビルド要件に `libclang-dev` と C コンパイラの記載が無く、初回ビルドが bindgen の libclang 未検出で失敗する。macOS の Xcode Command Line Tools の記載も無い。
- `README.md` から `docs/LINUX.md` への相対リンクは、`Cargo.toml` の `include` に `docs/` が含まれないため crates.io 上で切れる。
- ビルドコマンドと feature の説明が README と `docs/LINUX.md` で二重管理されている。`default-*` の個数は README が「1 つだけ」、`build.rs` は「高々 1 つ (0 も許容)」で、panic メッセージも `exactly one` と食い違う。
- `CHANGES.md` の `## develop` 内で MSRV が 1.95 と 1.93 の 2 通り書かれている。Windows の NUL バイトを含む `device_id` 拒否が変更履歴に記載されていない。「無期限にブロックすることをドキュメントに記載する」とあるが、記載は C ヘッダのみで公開 rustdoc に無い。
- `audio_coreaudio.m` / `audio_pulse.c` / `audio_pipewire.c` のコメントが「Rust 側の共通変換関数がバッファ末尾をゼロ埋めしている」と Rust の実装に依存しており、`CODEBASE.md` のモジュール責務の原則に反する。
- `audio_pipewire.c` の列挙コメント「monitor を除外する」は実装と一致しない (monitor を判定する条件が無い)。
- `Cargo.toml` の `nojson` のコメント「テスト用途で利用」は、実際に使っているのが examples だけである実態と一致しない。

## 設計方針

- README / docs のビルド要件・ランタイム依存・リンク・ビルドコマンド・`default-*` の記述を実装に合わせる。`default-*` は「高々 1 つ」に統一し、`build.rs` の panic メッセージも `at most one` にする。
- `CHANGES.md` の 1.93 記述を削除・統合し、Windows の `device_id` 変更を追記する。無期限ブロックの記載を公開 rustdoc へ移すか、CHANGES の表現を実態に合わせる。
- C のコメントは C バックエンドの責務内で完結する表現に直す。
- monitor を除外する実装を追加するか、コメントを実装に合わせる。
- `Cargo.toml` の `nojson` の用途コメントを、実際に使っている examples に合わせる。

## 完了条件

- README / docs / `CHANGES.md` / C コメント / `Cargo.toml` / `build.rs` の記述が実装と一致している。
- `default-*` の記述が README / docs で「高々 1 つ」、`build.rs` の panic メッセージで `at most one` に統一されている。
- `Cargo.toml` の `nojson` の用途コメントが examples の利用実態と一致している。
- crates.io で README の相対リンクが切れない。
- `cargo test --workspace` と `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` が通る。
