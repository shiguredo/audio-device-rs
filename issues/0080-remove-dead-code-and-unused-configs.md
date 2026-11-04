# 死にコードと未使用の設定を削除する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/remove-dead-code-and-unused-configs
- Polished: 2026-11-04

## 目的

レビューで見つかった死にコード・未使用の設定・重複した防御コードを削除し、あわせて実装と食い違うコメントや重複した doc を修正して、コードと設定の表面積を減らす。

`build.rs` の重複した `match target_os`、`.markdownlint.jsonc`、`Makefile` の `fuzz: fuzzing` は 0077 で扱うため、本 issue では扱わない。

## 現状

- `src/device.rs` の `AudioDevice` / `AudioDeviceList` への `unsafe impl Send` / `Sync` は、内側の型が既に `Send + Sync` のため何も変えない冗長な実装である。
- `AudioDeviceListInner::Wasapi` の `_inner` は `std::mem::take` 後は空の `Vec` を持つだけで何も保持していない。
- `audio_pulse.c` の `enumerate_state_callback` は state を取得するだけで何もせず、`pa_context_set_state_callback` への登録も無意味である。
- `audio_coreaudio.m` の `find_device_by_uid` は存在確認のためだけに全デバイスを列挙しており、戻り値は `kAudioObjectUnknown` かどうかの判定にしか使っていない。
- `capture_wasapi.rs` の `bytes_per_sample` の手書き match は `AudioFormat::bytes_per_sample` と重複しており、capture の `format` は常に S16 なので F32 の分岐は到達不能である。playback は既に `AudioFormat::bytes_per_sample` を使っている。
- C のデバイスアクセサの NULL 分岐は Rust から到達不能で、`device_type(NULL)` が INPUT を返す既定値も誤解を招く。
- `device_wasapi.rs` の `SendHandle::into_inner` / `SendPtr::into_inner` は、フィールドが既に `pub(crate)` のため二重公開になっている。
- `Cargo.toml` の `Win32_Media_KernelStreaming` feature はソースから参照されていない。
- `canary.py` の `git_commit_version` と `git_operations_after_build` のコメントはどちらも「git コミット、タグ、プッシュを実行」で、実際の処理と一致しない。後者のコメントは前者のコピーである。
- `src/playback.rs` の `AudioPlayback::new` の doc は、コールバック引数の説明を同一 doc 内で二重に書いている。

## 設計方針

- 冗長な `unsafe impl` と空の `_inner` を削除し、`AudioDeviceListInner::Wasapi { devices }` に整理する。
- 空のコールバックと登録、C のデバイスアクセサの NULL 分岐、未使用の feature を削除する。C のデバイスアクセサは非 NULL を前提とし、Rust 側の `NonNull` で保証する。削除後は Windows / Linux / macOS のビルドを確認する。
- `find_device_by_uid` を削除し、デバイスの存在確認は `AudioQueueSetProperty` の成否で行う。存在しない `device_id` を `Error::DeviceNotFound` にする変更は 0084 の範囲とする。0084 の現状にある `find_device_by_uid` の記述は、本 issue の削除後に更新が必要である。
- `bytes_per_sample` は `AudioFormat::bytes_per_sample` に置き換える。
- `SendHandle` / `SendPtr` の `into_inner` とフィールドの二重公開を解消する。フィールドが `pub(crate)` のままなら `into_inner` を削除し、フィールドを private にするなら `into_inner` を残す。
- `canary.py` の `git_commit_version` と `git_operations_after_build` のコメントをそれぞれの実装に合わせて修正し、`src/playback.rs` の重複した引数説明を削除する。

## 完了条件

- 現状に列挙した各項目が解消されている。
  - `AudioDevice` / `AudioDeviceList` の冗長な `unsafe impl Send` / `Sync` が削除されている。
  - `AudioDeviceListInner::Wasapi` から `_inner` が削除されている。
  - `audio_pulse.c` の `enumerate_state_callback` と、列挙時の `pa_context_set_state_callback` への登録が削除されている。
  - `audio_coreaudio.m` の `find_device_by_uid` が削除され、デバイスの存在確認が `AudioQueueSetProperty` の成否で行われる。
  - `capture_wasapi.rs` の `bytes_per_sample` の手書き match が `AudioFormat::bytes_per_sample` に置き換わっている。
  - C のデバイスアクセサから NULL 分岐が削除されている。
  - `SendHandle` / `SendPtr` の `into_inner` とフィールドの二重公開が解消されている。
  - `Cargo.toml` の `Win32_Media_KernelStreaming` feature とその用途コメントが削除されている。
  - `canary.py` の `git_commit_version` と `git_operations_after_build` のコメントがそれぞれの実装と一致している。
  - `src/playback.rs` のコールバック引数説明の重複が削除されている。
- Windows / Linux / macOS のビルドが通る。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が通る。
