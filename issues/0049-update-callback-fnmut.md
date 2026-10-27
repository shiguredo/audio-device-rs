# コールバック境界を FnMut + Send にする

- Created: 2026-10-27
- Completed: {YYYY-MM-DD}
- Branch: feature/update-callback-fnmut
- Polished: {YYYY-MM-DD}

## 目的

`AudioCapture` / `AudioPlayback` のコールバックを `Fn` から `FnMut` に変更し、コールバックが保持する状態を `Cell` / `RefCell` / `Atomic` / `Mutex` なしで変更できるようにする。

`Fn` のままでも、`Sync` を外す issue (0048) の後は `Cell` / `RefCell` を使って状態を持てる。ただし、コールバックが本質的に持つ「呼び出しごとに更新される状態」を表現するには `FnMut` のほうが自然であり、次のようなコードがそのまま書ける。

- 再生位置を `let mut position = 0u64;` で持つ
- `VecDeque` / `Vec` の作業バッファをそのまま持つ
- フィルタ、リサンプラ、デコーダなど `&mut self` を取る API をそのまま呼ぶ

実際に `examples/playback_sine.rs` は位相保持に `AtomicU64` を使っている。`FnMut` にすると `u64` をそのまま更新でき、`AtomicU64` と `Ordering` が不要になる。

`Fn` クロージャは `FnMut` を満たすため、この変更は後方互換である。`Sync` は引き続き要求しない。コールバックはバックエンドの専用スレッドから直列に呼ばれ、`stop` は実行中コールバックの完了を待つ。

## 前提

- 0048 の完了が前提である。0048 でコールバック境界を `Send` のみにし、WASAPI の `Arc<WasapiCaptureContext>` を `Box` + `SendPtr` に分離する
- 0048 の `audio.h` と safety コメントの「コールバックは単一のバックエンドスレッドから直列に呼ばれる」を根拠にする
- `FnMut` にしても `Box<dyn FnMut + Send>` は `!Sync` なので、WASAPI の `Arc` を戻すことはできない。0048 の構造を維持する

## 現状

- 0048 完了時点の公開 API は `F: Fn(...) + Send + 'static` を要求する
- `CaptureContext` / `PlaybackContext` は `Box<dyn Fn(...) + Send>` を保持し、C のコールバック関数は `&CaptureContext` / `&PlaybackContext` を作って呼ぶ
- `Fn` で状態を変更するには `Cell` / `RefCell` / `Atomic` / `Mutex` などの interior mutability が必要
- `examples/playback_sine.rs` は `AtomicU64`、`tests/test_device_timing.rs` は `Arc<AtomicU64>` をコールバックに渡している
- `AudioCapture` / `AudioPlayback` は `Sync` ではなく、`AudioCaptureSession` は別スレッドへ move できる

## 設計方針

### 公開 API

`F: FnMut(...) + Send + 'static` に変更する。`Sync` は要求しない。`Fn` クロージャは `FnMut` を満たすため、既存の呼び出し側はそのままコンパイルできる。

### FFI バックエンド

`CaptureContext` / `PlaybackContext` の callback を `UnsafeCell<Box<dyn FnMut(...) + Send>>` として保持し、`frame_callback` / `playback_callback` で `&mut *cell.get()` を取り出して呼ぶ。`CaptureContext` 自体は `&` でアクセスしたまま、callback だけを interior mutability で更新する。

- `UnsafeCell` を使う理由は、`stop` が別スレッドで `&mut AudioCapture` を持つ間にコールバックが動く経路があり、`&mut CaptureContext` を作るより、外側は共有参照のままにしたほうが aliasing の説明が単純になるため
- safety コメントには「コールバックは専用スレッドから直列に呼ばれ、同時呼び出しも再入もない」「`stop` は実行中コールバックの完了を待つ」と書く
- `*mut CaptureContext` を直接 `&mut` にキャストする方法もあるが、`UnsafeCell` のほうが意図が明確で、外側の参照を共有のまま保てる

### WASAPI

0048 の `Box<...>` + `SendPtr<*mut ...>` の構造を維持し、スレッド内で `UnsafeCell` から `&mut callback` を取って呼ぶ。`running` は `Arc<AtomicBool>` のまま。`Mutex` は使わない。

### ドキュメント

- 公開 API の doc に「コールバックはバックエンドの専用スレッドから直列に呼ばれ、状態を保持できる」と書く
- `audio.h` には 0048 で追加した直列の契約を維持する
- `README` の再生例を `FnMut` で状態を持つ形に更新する

## 完了条件

- `AudioCapture` / `AudioPlayback` のコールバック境界が `FnMut + Send` になること
- `CaptureContext` / `PlaybackContext` の callback が `Box<dyn FnMut + Send>` になること
- `Fn` クロージャを渡す既存コードがそのままコンパイルできること
- `u64` / `VecDeque` などの可変状態を `Cell` / `RefCell` / `Atomic` / `Mutex` なしで保持するコールバックがコンパイルできること
- `examples/playback_sine.rs` が `AtomicU64` を使わずに `u64` で位相を保持すること
- `AudioCapture` / `AudioPlayback` が `Sync` でないこと、`AudioCaptureSession` が別スレッドへ move できることが維持されること
- `cargo fmt --check` / `cargo clippy --workspace --all-targets` / `cargo test --workspace` が通ること
- `CHANGES.md` に [UPDATE] として記載すること

## テスト

- `tests/test_capture.rs` / `tests/test_playback.rs` に、`u64` を捕捉して更新する `FnMut` コールバックを渡すテストを追加する。デバイスが無い環境では `new` がエラーを返すため、`Ok` / `Err` のどちらでも成功扱いにする。目的は実行時動作ではなく、`FnMut` 境界と状態更新のコンパイル可否であることをコメントに書く
- `tests/session_lifecycle.rs` の「コールバックをブロックしたまま別スレッドから stop する」テストが引き続き通ることを確認する
- `examples/playback_sine.rs` を実行し、`AtomicU64` なしで再生位置が継続することを確認する
- WASAPI の `&mut` 経路は実機テストとコードレビューで確認する

## 後方互換

`Fn` クロージャは `FnMut` を満たすため、利用者側のコードは変更不要である。`Sync` を要求しない点も 0048 から変わらない。`AudioCapture` / `AudioPlayback` の `Send` / `Sync` の性質は変わらない。

## 設計上のトレードオフ

- `FnMut` は `&mut` の排他アクセスを要求する。コールバックが同時に呼ばれないこと、再入しないことが安全性の前提になる
- 将来バックエンドがコールバックを並行に呼ぶ必要が出た場合は、`Mutex` を入れるか API を再設計する必要がある
- `FnMut` にしても `Arc<Context>` は使えない (`Box<dyn FnMut + Send>` は `!Sync`)。0048 の構造変更が前提である
- 性能面のデメリットはほぼない。`Box<dyn FnMut>` の動的ディスパッチは `Box<dyn Fn>` と同じで、状態アクセスは atomics より素直である

## 参考

- cpal のデータコールバックは `FnMut(...) + Send + 'static` を要求している
- video-device-rs は `Fn + Send` のままで、`FnMut` にはしていない。本 issue は video-device-rs の追従ではなく、audio-device-rs のコールバックを状態付きで扱うための拡張である
