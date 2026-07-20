# playback_wasapi.rs のコールバック順序修正

- Priority: Medium
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-playback-wasapi-callback-order
- Polished: {YYYY-MM-DD}

## 目的

`src/playback_wasapi.rs` の `playback_thread_func` で、ユーザーコールバックが `GetBuffer` より前に呼ばれている問題を修正する。

## 優先度根拠

`GetBuffer` が失敗した場合、コールバックから取得したフレームデータが破棄される。コールバックに副作用（リードポインタの前進、キューからのポップ等）がある場合、オーディオデータが不可逆的に失われる。

## 現状

src/playback_wasapi.rs:218-232:

```rust
// まずコールバックを呼ぶ
let frame_opt = (context.callback)(frames_available as i32, channels, sample_rate);

// その後に GetBuffer
let data_ptr = match unsafe { render_client.GetBuffer(frames_available) } {
    Ok(p) => p,
    Err(_) => continue, // GetBuffer 失敗 → frame_opt は破棄される
};
```

## 完了条件

- `GetBuffer` を先に呼び、バッファ確保に成功してからコールバックが呼ばれる
- `cargo clippy` / `cargo test` が通る

## 解決方法

`GetBuffer` とコールバックの呼び出し順序を入れ替える。
