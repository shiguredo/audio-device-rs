# 死にコードと未使用の設定を削除する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/remove-dead-code-and-unused-configs
- Polished: {YYYY-MM-DD}

## 目的

レビューで見つかった死にコード・未使用の設定・重複した防御コードを削除し、コードと設定の表面積を減らす。

## 現状

- `src/device.rs` の `AudioDevice` / `AudioDeviceList` への `unsafe impl Send` / `Sync` は、内側の型が既に `Send + Sync` のため何も変えない冗長な実装である。
- `AudioDeviceListInner::Wasapi` の `_inner` は `std::mem::take` 後は空の `Vec` を持つだけで何も保持していない。
- `audio_pulse.c` の `enumerate_state_callback` は state を取得するだけで何もせず、登録も無意味である。
- `build.rs` の 2 回目の feature 判定と `_ => panic!("Unsupported target OS: ...")` は到達不能である (1 回目の match で既に panic する)。
- `audio_coreaudio.m` の `find_device_by_uid` は存在確認のためだけに全デバイスを列挙しており、戻り値の `AudioDeviceID` を使っていない。
- `capture_wasapi.rs` の `bytes_per_sample` の手書き match は `AudioFormat::bytes_per_sample` と重複しており、capture の `format` は常に S16、playback は常に F32 なのでもう一方の分岐は到達不能である。
- C のデバイスアクセサの NULL 分岐は Rust から到達不能で、`device_type(NULL)` が INPUT を返す既定値も誤解を招く。
- `device_wasapi.rs` の `SendHandle::into_inner` / `SendPtr::into_inner` は、フィールドが既に `pub(crate)` のため二重公開になっている。
- `Cargo.toml` の `Win32_Media_KernelStreaming` feature はソースから参照されていない。
- `.markdownlint.jsonc` はどのフックからも参照されていない。`Makefile` の `fuzz: fuzzing` は実体の無い別名。`canary.py` のタグ付け関数のコメントはコミット関数のコメントのコピーで実装と一致しない。`src/playback.rs` のコールバック引数説明は同一 doc 内で二重に書かれている。

## 設計方針

- 冗長な `unsafe impl` と空の `_inner` を削除し、`AudioDeviceListInner::Wasapi { devices }` に整理する。
- 空のコールバックと登録、到達不能な match / 分岐、未使用の feature を削除する。削除後は Windows / Linux のビルドを確認する。
- `find_device_by_uid` を削除し、`AudioQueueSetProperty` の失敗をデバイス不在として扱う。
- `bytes_per_sample` は共通関数に置き換える。
- 未参照の markdownlint 設定は、フックを追加するか削除する (フック追加は別 issue)。Makefile の別名、重複コメント、doc の重複を削除する。
- `SendHandle` / `SendPtr` の `into_inner` はフィールド可視性とどちらかに一本化する。

## 完了条件

- 列挙した死にコードと未使用設定が削除されている。
- Windows / Linux / macOS のビルドが通る。
- `cargo fmt` / `cargo clippy --workspace --all-targets` / `cargo test --workspace` が通る。
