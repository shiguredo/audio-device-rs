# examples のビルド条件とエラー処理を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-examples-ci-build-and-error-handling
- Polished: {YYYY-MM-DD}

## 目的

examples が一部の feature 構成でビルドできず、またデバイス情報の取得エラーを無言で握り潰している問題を修正する。

## 現状

- `examples/device_list.rs` / `examples/device_info.rs` / `examples/playback_sine.rs` の `main` は `enable_coreaudio` / `enable_pulse` / `enable_pipewire` / `enable_wasapi` のいずれかで gate されているが、呼び出している `AudioDeviceList::enumerate` や `AudioPlayback::new` は `enable_default_*` でしかコンパイルされない。
- そのため `--no-default-features --features pipewire` のように「明示 API だけを使うバックエンド」を有効にすると、`main` はコンパイル対象になるが呼び出す API が存在せず E0599 でビルドできない。`CHANGES.md` の「examples が `--no-default-features` でビルドできない問題を修正する」はバックエンド 0 個の場合だけを修正している。
- `examples/device_list.rs` と `examples/device_info.rs` は `device.name().unwrap_or_default()` / `device.unique_id().unwrap_or_default()` でエラーを空文字に握り潰している。生成した JSON の `name` / `unique_id` が空になり、CI の `jq` チェックは原因不明のまま失敗する。
- `examples/playback_sine.rs` の `(requested_frames * channels) as usize` は i32 の乗算であり、極端な要求値でオーバーフローし得る。

## 設計方針

- examples の `main` を `enable_default_*` で gate するか、有効なバックエンドに応じて明示 API を呼ぶ実装に切り替える。`--no-default-features --features pipewire` で `cargo build --examples` が通ることを確認する。
- デバイス名・ID の取得エラーは `eprintln!` するか、JSON に `null` を出す。エラーを空文字で隠さない。
- 要求フレーム数とチャンネル数の積は `usize` で計算するか、`checked_mul` で検証する。
- examples の修正後に CI の feature 組合せでビルドを検証する (CI 側の対応は別 issue)。

## 完了条件

- `cargo build --examples --no-default-features` と `--no-default-features --features pipewire` / `--features pulse` で examples がビルドできる。
- examples がデバイス情報の取得エラーを無言で握り潰さない。
- 要求フレーム数の乗算がオーバーフローしない。
- `cargo fmt` / `cargo clippy --workspace --all-targets` が通る。
