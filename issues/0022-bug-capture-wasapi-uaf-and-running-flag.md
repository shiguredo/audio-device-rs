# capture_wasapi.rs の Use-After-Free と running フラグ修正

- Priority: High
- Created: 2026-07-20
- Completed: {YYYY-MM-DD}
- Model: Qwen 3
- Branch: feature/fix-capture-wasapi-uaf-and-running-flag
- Polished: 2026-07-21

## 目的

`src/capture_wasapi.rs` に存在する 3 つのバグを修正する。

1. `mix_format` の Use-After-Free（UAF）: `CoTaskMemFree` で解放したポインタを `audio_client.Initialize()` に渡している
2. `start()` のスレッド生成失敗時に `running` フラグが `true` のまま残る
3. `start()` のスレッド生成失敗時に `Error::SessionCreateFailed` を返している（正しくは `Error::SessionStartFailed`）

## 優先度根拠

UAF は未定義動作であり、クラッシュ・メモリ破壊を引き起こす経路。`running` フラグの残留は `stop()` を介さない再 `start()` をサイレントに無効化する。

## 現状

### UAF（src/capture_wasapi.rs:70-83）

`capture_wasapi.rs:70` で `CoTaskMemFree` により `mix_format` を解放した後、`capture_wasapi.rs:74-83` の `Initialize` 呼び出しで解放済みポインタを使用している。

```rust
CoTaskMemFree(Some(mix_format as *const _)); // 70 行目: 解放

// オーディオクライアントを初期化（10ms バッファ）
let buffer_duration: i64 = 100_000; // 10ms in 100-nanosecond units
audio_client
    .Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        buffer_duration,
        0,
        mix_format, // 80 行目: 解放済みポインタを使用（UAF）
        Some(std::ptr::null()),
    )
```

なお 62 行目の `let wave_format = &*mix_format` も CoTaskMem 領域への参照であり、`CoTaskMemFree` 以降はダングリングになる。これは `playback_wasapi.rs:71` にも存在する既存パターンであり本 issue の対象外。修正後のコードでは `CoTaskMemFree` 以降で `wave_format` を使用しないことを確認すること。

### running フラグ（src/capture_wasapi.rs:139-168）

`capture_wasapi.rs:140` で `running` を `true` に設定した後、`capture_wasapi.rs:151-168` の `spawn` が失敗しても `running` が `false` にリセットされない。

```rust
// 139 行目: コメントは「スレッド生成成功後に running フラグを立てる」とあるが実際には生成前
context.running.store(true, Ordering::Release); // 140 行目

// 142-148 行目: capture_client, event_handle, format 等のクローン処理
let capture_client = SendPtr(session.capture_client.clone());
let event_handle = SendHandle(session.event_handle);
// ...（中略）...
let context = Arc::clone(context); // 148 行目: context をシャドウイング

let handle = thread::Builder::new()
    .spawn(move || { ... })
    .map_err(|_| {
        // 163-168 行目: running が true のまま！
        unsafe { let _ = session.audio_client.Stop(); }
        Error::SessionCreateFailed // 167 行目
    })?;
```

### エラー型の不整合

`capture_wasapi.rs:167` の `spawn` 失敗時は `Error::SessionCreateFailed` を返すが、`playback_wasapi.rs:190` の同一パスは `Error::SessionStartFailed` を返す。

### 再現条件

- running フラグ残留: `start()` でスレッド生成が失敗した後に `stop()` を介さず再度 `start()` を呼ぶと、`running` が `true` のため `Ok(())` が即座に返りキャプチャが開始されない

## 設計方針

`playback_wasapi.rs` の既存の正しいパターンに揃える。`IAudioClient::Initialize` は `WAVEFORMATEX` を読み取るが所有権は取らないため、呼び出し側は `Initialize` が戻るまでメモリを有効に保つ必要がある。

## 完了条件

- `capture_wasapi.rs` の `new()` で `CoTaskMemFree` が `Initialize` の後に呼ばれる
- `start()` のスレッド生成失敗時に `running` が `false` にリセットされる
- `start()` の `spawn` 失敗時のエラー型を `Error::SessionStartFailed` に揃える
- `cargo clippy --target x86_64-pc-windows-msvc` が通る（WASAPI は Windows 専用のため、macOS 上の `cargo test` ではこのコードパスは検証されない。クロスコンパイルでの静的検証を行う）
- 実機テストが不可能な場合は `playback_wasapi.rs` との差分対照によるコードレビューで担保する

## 解決方法

1. `new()` の 70 行目にある `CoTaskMemFree` を削除し、`Initialize` の結果を一時変数 `init_result` に受け取り、`Initialize` の後に `CoTaskMemFree` を呼び、その後に `init_result` をエラーチェックする（`playback_wasapi.rs:80-92` と同じ順序）。`buffer_duration` 変数は capture 側の既存パターンとして維持する（playback はインラインだが、差分対照では順序のみを比較する）
2. `start()` の 148 行目 `let context = Arc::clone(context)` を `let thread_context = Arc::clone(context)` に変更し、160 行目の `capture_thread_func(... context)` も `capture_thread_func(... thread_context)` に変更する（`playback_wasapi.rs:168,182` と同じ別名化。これにより `map_err` クロージャ内から元の `context` 参照が利用可能になる）
3. `start()` の `spawn` 失敗時のエラーパスに `context.running.store(false, Ordering::Release)` を追加する（`audio_client.Stop()` の前に挿入。`playback_wasapi.rs:186` と同じ順序）。手順 2 のシャドウイング解除が未適用だと `context` が move 済みでコンパイルできないため、手順 2 を先に適用すること
4. `start()` の `spawn` 失敗時のエラー型を `Error::SessionCreateFailed` から `Error::SessionStartFailed` に変更する
5. 139 行目のコメントを「スレッド生成前に running フラグを立てる」に修正する（`playback_wasapi.rs:164` と同一文案）

## 後方互換

公開 API（`AudioCapture`）のシグネチャに変更はない。`start()` 失敗時に返るエラーバリアントが `SessionCreateFailed` から `SessionStartFailed` に変わる（本来返るべき値への修正）。`Error` は `pub enum`（`src/error.rs`）であり `lib.rs` で `pub use` されているため、`Error` を match している下流コードは影響を受けうる
