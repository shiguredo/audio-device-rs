# CI の feature 組合せとプラットフォーム検証を補強する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-ci-feature-matrix
- Polished: {YYYY-MM-DD}

## 目的

README が advertise する feature 構成と、プラットフォームごとのテスト条件が CI で検証されるようにする。

## 現状

- CI の matrix は「Ubuntu (pulse) = 既定」「Ubuntu (pipewire) = `--no-default-features --features default-pipewire`」の 2 通りだけで、README と `docs/LINUX.md` が案内する `--features pipewire` (pulse と pipewire の共存) が一度もビルドされない。共存時のみ 2 つの Ops 定数と 2 ヘッダ分の bindings が同時にコンパイルされる。
- バックエンドを 0 個にする `--no-default-features` のみのビルドも CI に無く、examples の修正が退行しても検出できない。
- CI の macOS メインジョブは `cargo test --workspace` をそのまま実行するが、device-test の macOS ジョブだけが `--skip capture_timestamps_track_audio_time` を付けている。同じ制約下で挙動が不統一。
- GitHub-hosted の CI は PulseAudio を `-n --load=module-native-protocol-unix` で、PipeWire を bare で起動するためデバイスが 0 件になり、実機系テストが成功扱いで早期 return する。null sink / null source を用意すれば検証範囲を増やせる。
- 同一リポジトリ内で action の固定方法が混在している。`actions/checkout` と `crates-io-auth-action` は SHA 固定 + バージョンコメント、`shiguredo/github-actions` は `@main`。
- Rust toolchain `"1.95"` が `rust-toolchain.toml` と CI の 2 箇所で管理されている。runner 選定理由 (`ubuntu-24.04` / `ubuntu-24.04-arm`) もコメントが無い。

## 設計方針

- matrix に `--features pipewire` (共存) と `--no-default-features` のみのジョブを追加する。
- macOS のテスト条件をメイン CI と device-test で揃えるか、揃えない理由をコメントで示す。
- CI に null sink / null source を用意し、デバイス 0 件スキップを減らす。
- action の固定方法を統一する (SHA 固定 + バージョンコメント)。toolchain は `rust-toolchain.toml` を正本とし、CI 側の二重管理を解消する。
- runner 選定理由をコメントで残す。

## 完了条件

- 共存構成とバックエンド 0 個のビルドが CI で検証される。
- macOS のテスト条件がジョブ間で説明可能な形になっている。
- 実機系テストのスキップが減り、CI で検証される経路が増えている。
- action の固定方法と toolchain の管理箇所が統一されている。
- CI が全ジョブ成功する。
