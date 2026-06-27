# Windows でスレッド生成失敗時にオーディオエンジンが停止しない

- Priority: Medium
- Created: 2026-06-28
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-windows-audio-client-running-on-spawn-failure
- Polished: {YYYY-MM-DD}

## 目的

Windows 版の `start()` でスレッド生成に失敗した場合でも、オーディオクライアントを確実に停止する。

## 優先度根拠

`thread::spawn` 失敗は稀だが、失敗した場合 `audio_client.Start()` 済みのエンジンが停止しないまま残る。`running` フラグが `false` のままなので、後続の `stop()` も何もしない。これはリソースリークおよび内部状態の不整合を引き起こす。

## 現状

`src/capture_windows.rs` の `start()` は以下の順序になっている。

1. `audio_client.Start()` を呼ぶ
2. `thread::Builder::new().spawn(...)` を呼ぶ
3. 成功した場合に `running` を `true` にする

`thread::spawn` が失敗すると、`running` は `false` のまま、`audio_client` は停止されない。`src/playback_windows.rs` でも同様に、`audio_client.Start()` 後に `thread::spawn` を呼んでおり、失敗時に `Stop()` が呼ばれていない。

## 設計方針

`thread::spawn` のエラーハンドリングで、確実に `audio_client.Stop()` を呼び出す。キャプチャ側と再生側の両方を修正する。

## 完了条件

- `start()` 中に `thread::spawn` が失敗しても `audio_client.Stop()` が呼ばれること
- 正常系の動作が変わらないこと
- `running` フラグが整合した状態を保つこと

## 解決方法

`src/capture_windows.rs` と `src/playback_windows.rs` の `start()` 内、`thread::spawn` の `map_err` クロージャで `session.audio_client.Stop()` を呼び出す。

```rust
let handle = thread::Builder::new()
    .name("audio-capture".into())
    .spawn(move || { ... })
    .map_err(|_| {
        unsafe {
            let _ = session.audio_client.Stop();
        }
        Error::SessionStartFailed
    })?;
```

再生側も同様に、`context.running.store(false, Ordering::Release)` の前後または代わりに `audio_client.Stop()` を呼び出す。
