# CI の feature 組合せ・プラットフォーム検証・設定の一貫性を整備する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-ci-feature-matrix
- Polished: 2026-11-04

## 目的

README と `docs/LINUX.md` が案内する feature 構成、およびプラットフォームごとのテスト条件が CI で検証されるようにする。
あわせて、CI の action の固定方法・Rust toolchain の管理・runner 選定理由を整え、CI の意図を追える状態にする。

## 現状

- `ci` ジョブの Linux の feature 構成は「既定 (pulse)」と「`--no-default-features --features default-pipewire`」の 2 通りだけで、README と `docs/LINUX.md` が案内する `--features pipewire` (pulse と pipewire の共存) が一度もビルドされない。共存時は `DeviceOps` / `CaptureOps` / `PlaybackOps` の pulse と pipewire の実装 (計 6 個の `OPS_*` 定数) と、2 ヘッダ分の bindings が同時にコンパイルされる。
- バックエンドを 0 個にする `--no-default-features` のみのビルドも CI に無い。`cargo check --workspace` は examples を対象にしないため、`--all-targets` を付けないとバックエンド 0 個時の examples の fallback `main` の退行を検出できない。
- 0062 は examples の明示バックエンド構成 (`--no-default-features --features pipewire` / `pulse` / `coreaudio` / `wasapi`) の CI 検証を 0075 に委ねているが、現在の `ci` ジョブの matrix にはこれらの構成が無い。
- `ci` ジョブの macOS は `cargo test --workspace` をそのまま実行するが、`device-test` の macOS は `--skip capture_timestamps_track_audio_time` を付けている。`device-test` のコメントは「macOS の CI ではマイクへのアクセス許可がないため」としており、`ci` ジョブの macOS も同じ制約を受ける。
- GitHub-hosted の `ci` ジョブは PulseAudio を `-n --load=module-native-protocol-unix` で、PipeWire を bare で起動するためデバイスが 0 件になり、`tests/session_lifecycle.rs` と `tests/test_device_timing.rs` の実機系テストが早期 return する。null sink / null source を用意すれば検証範囲を増やせるが、0046 で PulseAudio の `auto_null` を外した理由として「2 秒ブロックで再生コールバックが継続しない」ことが記録されている。`tests/test_device_timing.rs` の再生タイミング検証は 1.8 秒以上・10 点以上の測定を要求するため、null sink をそのまま全テストに使うと失敗する。
- 同一リポジトリ内で action の固定方法が混在している。`actions/checkout` と `crates-io-auth-action` は SHA 固定 + バージョンコメント、`shiguredo/github-actions` は `@main`。
- Rust toolchain `"1.95"` が `rust-toolchain.toml` と CI の `rust-cache` 入力の 2 箇所に書かれている。MSRV は `Cargo.toml` の `rust-version` にも `"1.95"` と書かれており、`rust-toolchain.toml` の `channel` と一致させる必要がある。`rust-cache` の `toolchain` 入力はキャッシュキーにのみ使われ、実際のツールチェーンは `rustup show` が `rust-toolchain.toml` から選ぶ。runner 選定理由 (`ubuntu-24.04` / `ubuntu-24.04-arm`) もコメントが無い。

## 設計方針

- `ci` ジョブの matrix に、Linux の共存構成 (`--features pipewire`)、Linux のバックエンド 0 個 (`--no-default-features`)、および 0062 が求める明示バックエンド構成 (`--no-default-features --features pipewire` / `pulse`、macOS の `--no-default-features --features coreaudio`、Windows の `--no-default-features --features wasapi`) を追加する。
- 共存構成のジョブでは `libpulse-dev` と `libpipewire-0.3-dev` の両方をインストールする。現在の依存インストール条件は `pipewire` を含むと pulse 側を入れないため、条件を見直す。
- バックエンド 0 個のジョブではオーディオサーバを起動せず、`libpulse-dev` / `libpipewire-0.3-dev` も不要にする。現在の Linux テストステップは `audio-server: none` を扱っていないため、`none` の分岐を追加するか、サーバ起動ステップをスキップする。
- バックエンド 0 個と明示バックエンド構成のジョブでは、`cargo check --workspace --all-targets` (または `cargo test --workspace`) で examples を含めてビルドを検証する。`cargo check --workspace` だけでは examples が対象にならない。
- macOS のテスト条件は、`ci` ジョブの macOS にも `device-test` と同じ `--skip capture_timestamps_track_audio_time` を付け、理由をコメントで示す。`device-test` の macOS で実機マイクが使える場合は、skip を外す理由もコメントに書く。
- `ci` ジョブの Linux で null sink / null source を用意する。PulseAudio は `module-null-sink` と `module-virtual-source`、PipeWire は `support.null-audio-sink` の adapter を使う。`module-null-sink` は既定で 2 秒ブロックだが `norewinds=1` を指定すると 50 ms ブロックになる。0046 の 2 秒ブロックの制約を踏まえ、`norewinds=1` を指定するか、再生タイミング検証 (`playback_callbacks_track_audio_time`) は null sink では実行しない。対象を限定する場合は、その理由を CI のコメントに残す。
- action の固定方法を SHA 固定 + バージョンコメントに統一する。`shiguredo/github-actions` の action も SHA 固定にする。
- `rust-cache` の `toolchain` 入力は削除するか、`rust-toolchain.toml` の値と一致させる。実際のツールチェーンは `rustup show` が `rust-toolchain.toml` から選ぶため、CI 側で別の値を持たない。
- runner 選定理由 (`ubuntu-24.04` / `ubuntu-24.04-arm`) をコメントで残す。`ubuntu-slim` を採用しない理由 (rustup や apt で入れる依存の都合) も含める。

## 完了条件

- 共存構成、バックエンド 0 個、および明示バックエンド構成 (Linux の pipewire / pulse、macOS の coreaudio、Windows の wasapi) のビルドが CI で検証される。
- バックエンド 0 個と明示バックエンド構成のジョブで、examples を含む全ターゲットのビルドが検証される。
- macOS のテスト条件がジョブ間で説明可能な形になっている。
- null sink / null source により、CI で検証される実機系テストの経路が増えている。再生タイミング検証を除外する場合は、その理由が CI のコメントに残っている。
- action の固定方法と toolchain の管理箇所が統一されている。
- `ubuntu-24.04` / `ubuntu-24.04-arm` の runner 選定理由がコメントで説明されている。
- CI が全ジョブ成功する。
