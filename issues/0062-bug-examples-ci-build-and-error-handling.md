# examples のビルド条件とエラー処理、乗算のオーバーフローを修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-examples-ci-build-and-error-handling
- Polished: 2026-11-03

## 目的

examples が一部の feature 構成でビルドできず、デバイス情報の取得エラーを無言で握り潰し、再生サンプルの乗算がオーバーフローし得る問題を修正する。

## 現状

- `examples/device_list.rs` / `examples/device_info.rs` / `examples/playback_sine.rs` の `main` は `enable_coreaudio` / `enable_pulse` / `enable_pipewire` / `enable_wasapi` のいずれかで gate されているが、呼び出している `AudioDeviceList::enumerate` や `AudioPlayback::new` は `enable_default_*` でしかコンパイルされない。
- そのため `--no-default-features --features pipewire` のように「明示 API だけを使うバックエンド」を有効にすると、`main` はコンパイル対象になるが呼び出す API が存在せず E0599 でビルドできない。`--no-default-features --features pulse` (Linux)、`--no-default-features --features coreaudio` (macOS)、`--no-default-features --features wasapi` (Windows) でも同じ問題が起きる。`CHANGES.md` の「examples が `--no-default-features` でビルドできない問題を修正する」はバックエンド 0 個の場合だけを修正している。
- `examples/device_list.rs` と `examples/device_info.rs` は `device.name().unwrap_or_default()` / `device.unique_id().unwrap_or_default()` でエラーを空文字に握り潰している。生成した JSON の `name` / `unique_id` が空になり、CI の `jq` チェック (`.devices | all(.name | length > 0)` など) は原因不明のまま失敗する。
- `examples/playback_sine.rs` の `(requested_frames * channels) as usize` は i32 の乗算であり、極端な要求値でオーバーフローし得る。デバッグビルドではパニックし、リリースビルドでは巻き戻る。

## 設計方針

- examples はデフォルトバックエンドのデモであるため、`main` を `enable_default_*` で、fallback の `main` を `not(any(enable_default_*))` で gate する。`enable_default_*` が無い場合は、fallback の `main` が「default バックエンドが有効でない」旨の英語メッセージを出して終了する。examples の doc コメントも、default バックエンドが有効な場合に動作する旨に更新する。`--no-default-features --features pipewire` で `cargo build --examples` が通ることを確認する。
- デバイス名・ID の取得エラーは `eprintln!` で英語のメッセージを出し、JSON には `null` を出す。エラーを空文字で隠さない。
- 要求フレーム数とチャンネル数の積は `usize` に変換してから `checked_mul` で検証し、失敗時はコールバックで `None` を返す。
- examples の修正後に CI の feature 組合せでビルドを検証する。CI 側の対応は 0075 で行い、0075 で examples をビルドする CI ジョブに `--no-default-features --features pipewire` を追加する。macOS / Windows の明示バックエンド構成も同様に追加する。

## 完了条件

- Linux で `cargo build --examples --no-default-features`、`cargo build --examples --no-default-features --features pipewire`、`cargo build --examples --no-default-features --features pulse` が通る。
- macOS で `cargo build --examples --no-default-features --features coreaudio`、Windows で `cargo build --examples --no-default-features --features wasapi` が通る。
- examples がデバイス情報の取得エラーを無言で握り潰さない (エラーを英語で `eprintln!` し、JSON の `name` / `unique_id` は `null` にする)。
- 要求フレーム数の乗算がオーバーフローしない (`checked_mul` で検証し、失敗時は `None` を返す)。
- `cargo fmt --all -- --check` と `cargo clippy --workspace --all-targets -- -D warnings` が通る。
