# playback_wasapi.rs のコールバック順序修正

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-10-19
- Model: Qwen 3
- Branch: feature/fix-playback-wasapi-callback-order
- Polished: 2026-07-21

## 目的

`src/playback_wasapi.rs` の `playback_thread_func` で、ユーザーコールバックが `GetBuffer` より前に呼ばれている問題を修正する。

## 優先度根拠

`GetBuffer` が失敗した場合、コールバックから取得したフレームデータが破棄される。コールバックに副作用（リードポインタの前進、キューからのポップ等）がある場合、オーディオデータが不可逆的に失われる。コールバックのシグネチャは `Fn` だが、interior mutability（`Mutex`、`AtomicUsize`、チャネルの pop 等）で副作用は発生し得る。

## 現状

src/playback_wasapi.rs:293-300:

```rust
// ユーザーコールバックからフレームデータを取得する
let frame_opt = (context.callback)(frames_available as i32, channels, sample_rate);

// レンダリングバッファを取得する
let data_ptr = match unsafe { render_client.GetBuffer(frames_available) } {
    Ok(p) => p,
    Err(_) => continue,
};
```

294 行目でコールバックを呼んだ後、297-300 行目で `GetBuffer` を呼んでいる。`GetBuffer` が失敗すると `continue` でループに戻り、`frame_opt` は破棄される。

## 設計方針

`GetBuffer` を先に呼び、バッファ確保に成功してからコールバックを呼ぶ順序に変更する。既存の `if let Some(frame) = frame_opt { ... } else { ... }` 分岐構造は変更不要。`write_playback_frame_to_buffer` と `ReleaseBuffer` の呼び出しタイミングへの影響もない。バッファロック時間がコールバック実行分だけ延びるが、コールバックはフレーム生成のみで軽量であり、データ損失の防止を優先する。

## 完了条件

- `GetBuffer` を先に呼び、バッファ確保に成功してからコールバックが呼ばれる
- `cargo clippy --target x86_64-pc-windows-msvc` が通る（WASAPI は Windows 専用のため、macOS 上の `cargo test` ではこのコードパスは検証されない。クロスコンパイルでの静的検証を行う）
- WASAPI 固有の順序ロジックはテスト不能であるため、コードレビューで担保する

## 解決方法

`src/playback_wasapi.rs` の `playback_thread_func` で、レンダリングバッファの取得とユーザーコールバックの呼び出し順序を入れ替えた。

- `GetBuffer` を先に呼び、バッファの確保に成功してからコールバックを呼ぶようにした
- `GetBuffer` が失敗した場合はコールバックを呼ばずに `continue` するため、コールバックの副作用によるデータ損失は発生しない
- コールバックが `None` を返した場合は `ReleaseBuffer(frames_available, AUDCLNT_BUFFERFLAGS_SILENT)` で無音を書き込み、`Some(frame)` を返した場合は `write_playback_frame_to_buffer` で書き込んでから `ReleaseBuffer(frames_available, 0)` で解放する。この分岐構造は変更していない
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追記した

`cargo fmt --all --check`、`cargo clippy --target x86_64-pc-windows-msvc --workspace -- -D warnings`、`cargo test --workspace` がすべてパスすることを確認した。WASAPI 固有の順序ロジックはテストでは検証できないため、コードレビューで担保する。

## 後方互換

`pub(crate)` の内部実装のみの変更であり、公開 API（`AudioPlayback`）のシグネチャ・動作に変更はない
