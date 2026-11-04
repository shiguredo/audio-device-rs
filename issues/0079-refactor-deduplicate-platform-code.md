# バックエンド実装の重複と unsafe の根拠を整理する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-deduplicate-platform-code
- Polished: 2026-11-04

## 目的

バックエンド実装の重複と可視性・unsafe の根拠の不統一を整理し、バックエンド追加や変更のコストを下げる。

## 現状

- C の `struct AudioDevice`、5 個のアクセサ、`free_devices` が CoreAudio / PulseAudio / PipeWire の 3 バックエンドで関数名の接頭辞以外ほぼ同一で、150 行以上がコピーになっている。PulseAudio の `source_info_callback` と `sink_info_callback` も約 60 行ずつ重複している。
- 各バックエンドの capture / playback のセッション開始処理が約 100 行ずつほぼ同一 (PipeWire)、PulseAudio と CoreAudio も同型の二重実装を持つ。
- Rust の `FfiCaptureImpl` と `FfiPlaybackImpl`、`WasapiCaptureImpl` と `WasapiPlaybackImpl` は、フィールド構成・start / stop / Drop・アクセサがほぼ同一である。
- `CaptureContext` / `PlaybackContext` はモジュール内でしか使われないのに `pub(crate)`、`pub(crate)` 型のメソッドが `pub fn`、`AudioDeviceInner` / `AudioDeviceListInner` と tuple フィールドも不必要に公開されている。`*Impl` という命名は trait 実装でない具象型に `Impl` を付けており意図を表さない。`SendPtr` は生ポインタだけでなく COM インターフェースも運ぶ。`SendHandle` の doc コメント「MTA で初期化済みのため安全」は HANDLE には当てはまらない。
- `AudioDeviceListInner` は `_inner` を先、`devices` を後に宣言しているため、C 側デバイス配列の解放が `Vec<AudioDevice>` の破棄より先に走る。現状は `AudioDevice` に Drop が無いため実害は無いが、将来 Drop を足すと use-after-free になる。
- `AudioFrameOwned::as_s16` / `as_f32` の unsafe な再スライスは、`AudioFrame::as_s16` / `as_f32` の戻り値ライフタイムを `'a` に変更すれば不要になる。
- SAFETY コメントが unsafe ブロックごとに揃っていない。
- `examples/device_list.rs` と `examples/device_info.rs` のデバイス 1 件分の JSON 生成ブロックが完全に重複している。

## 設計方針

- C の共通構造体とアクセサ、`free_devices` を `audio.h` と共通実装へ寄せる。各バックエンド内で、capture / playback の開始処理を方向・category・コールバック群を引数に取る共通関数へ寄せる。
- Rust の `FfiCaptureImpl` / `FfiPlaybackImpl` と `WasapiCaptureImpl` / `WasapiPlaybackImpl` は、それぞれの組でセッション管理・スレッド管理・停止処理を共通化する。trait とマクロは使わない方針を維持する。
- 可視性を実際の利用範囲に合わせ、`*Impl` / `SendPtr` の命名と SAFETY コメント、`SendHandle` の doc コメントを実装に即した表現に直す。`AudioDeviceListInner` のフィールド破棄順は宣言順に依存しない形にする。
- `AudioFrameOwned` の unsafe を除去し、`AudioFrame` のアクセサのライフタイムを修正する。
- `examples/device_list.rs` と `examples/device_info.rs` のデバイス 1 件分の JSON 生成を共通モジュールへ寄せる。

## 完了条件

- C の `AudioDevice` 構造体・5 個のアクセサ・`free_devices`、PulseAudio の source / sink 列挙コールバック、各バックエンドの capture / playback セッション開始処理が共通実装に集約されている。
- Rust の `FfiCaptureImpl` / `FfiPlaybackImpl` と `WasapiCaptureImpl` / `WasapiPlaybackImpl` のセッション管理・スレッド管理・停止処理・アクセサの共通部分が共通実装に集約されている。
- 可視性・命名・SAFETY コメントおよび安全性の根拠となる doc コメントが実態と一致している。
- `AudioDeviceListInner` の破棄順が宣言順に依存せず、`AudioDevice` に `Drop` を追加しても C 側デバイス配列の解放後にアクセスしないことが保証されている。
- `AudioFrameOwned` の unsafe が不要になっている。
- `examples/device_list.rs` と `examples/device_info.rs` のデバイス 1 件分の JSON 生成が共通モジュールに集約されている。
- CI の全構成 (CoreAudio / PulseAudio / PipeWire / WASAPI) で `cargo fmt` / `cargo clippy --workspace --all-targets` / `cargo test --workspace` が通る。
