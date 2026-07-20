# extern "C" コールバックの panic 保護

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-extern-c-callback-panic
- Polished: {YYYY-MM-DD}

## 目的

`src/capture_ffi.rs` の `frame_callback` と `src/playback_ffi.rs` の `playback_callback` で、ユーザー提供のクロージャが panic した場合にプロセスが abort する問題を修正する。

## 優先度根拠

`extern "C"` 関数内での panic は FFI 境界でプロセスを abort させる。ユーザーコードの panic がライブラリ全体のクラッシュにつながる。

## 現状

src/capture_ffi.rs:161-183:

```rust
extern "C" fn frame_callback(...) {
    // ...
    (context.callback)(frame); // panic → プロセス abort
}
```

src/playback_ffi.rs:163-199:

```rust
extern "C" fn playback_callback(...) {
    // ...
    let Some(frame) = (context.callback)(frames, channels, sample_rate) else {
        return 0;
    }; // panic → プロセス abort
}
```

## 完了条件

- ユーザークロージャの panic がプロセス abort につながらない
- キャプチャ側は panic 時に early return、再生側は 0 を返す
- `cargo clippy` / `cargo test` が通る

## 解決方法

`std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ...))` でコールバック呼び出しを囲み、panic 時はキャプチャ側は early return、再生側は 0 を返す。
