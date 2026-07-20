# capture_wasapi.rs の Use-After-Free と running フラグ修正

- Priority: High
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-capture-wasapi-uaf-and-running-flag
- Polished: {YYYY-MM-DD}

## 目的

`src/capture_wasapi.rs` に存在する 2 つのバグを修正する。

1. `mix_format` の Use-After-Free（UAF）: `CoTaskMemFree` で解放したポインタを `audio_client.Initialize()` に渡している
2. `start()` のスレッド生成失敗時に `running` フラグが `true` のまま残る

## 優先度根拠

UAF は未定義動作であり、クラッシュ・メモリ破壊を引き起こす経路。`running` フラグの残留はキャプチャの再開始を不能にする。いずれも Windows プラットフォームの安定性に直結する。

## 現状

### UAF（src/capture_wasapi.rs:71-81）

```rust
CoTaskMemFree(Some(mix_format as *const _)); // 71 行目: 解放

audio_client
    .Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        buffer_duration,
        0,
        mix_format, // 解放済みポインタを使用（UAF）
        Some(std::ptr::null()),
    )
```

対照的に `playback_wasapi.rs` は `Initialize` の結果を一時変数に受け取り、その後に `CoTaskMemFree` を呼ぶ正しい順序になっている。

### running フラグ（src/capture_wasapi.rs:139-158）

```rust
// コメントは「スレッド生成成功後に running フラグを立てる」とあるが実際には生成前
context.running.store(true, Ordering::Release);

let handle = thread::Builder::new()
    .spawn(move || { ... })
    .map_err(|_| {
        // running が true のまま！
        unsafe { let _ = session.audio_client.Stop(); }
        Error::SessionCreateFailed
    })?;
```

対照的に `playback_wasapi.rs` はエラーパスで `context.running.store(false, Ordering::Release)` を正しく呼んでいる。

## 設計方針

`playback_wasapi.rs` の既存の正しいパターンに揃える。

## 完了条件

- `capture_wasapi.rs` の `new()` で `CoTaskMemFree` が `Initialize` の後に呼ばれる
- `start()` のスレッド生成失敗時に `running` が `false` にリセットされる
- `cargo clippy` / `cargo test` が通る

## 解決方法

1. `new()` で `Initialize` の結果を一時変数に受け取り、`CoTaskMemFree` の後にエラーチェックする（`playback_wasapi.rs` と同じパターン）
2. `start()` の `spawn` 失敗時のエラーパスに `context.running.store(false, Ordering::Release)` を追加する
3. コメントを実態に合わせて修正する
