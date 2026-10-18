# extern "C" コールバックの panic 保護

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-10-19
- Model: Qwen 3
- Branch: feature/fix-extern-c-callback-panic
- Polished: 2026-07-21

## 目的

`src/capture_ffi.rs` の `frame_callback` と `src/playback_ffi.rs` の `playback_callback` で、ユーザー提供のクロージャが panic した場合にプロセスが abort する問題を修正する。

## 優先度根拠

`extern "C"` 関数内での panic は FFI 境界でプロセスを abort させる。ユーザーコードの panic がライブラリ全体のクラッシュにつながる。

## 現状

### capture_ffi.rs:178-219（frame_callback）

```rust
extern "C" fn frame_callback(
    user_data: *mut c_void,
    data: *const c_void,
    frames: i32,
    channels: i32,
    sample_rate: i32,
    format: i32,
    timestamp_us: i64,
) {
    // ...
    (context.callback)(frame); // 218 行目: panic → プロセス abort
}
```

### playback_ffi.rs:190-234（playback_callback）

```rust
extern "C" fn playback_callback(
    user_data: *mut c_void,
    buffer: *mut c_void,
    frames: i32,
    channels: i32,
    sample_rate: i32,
    format: i32,
) -> i32 {
    // ...
    let Some(frame) = (context.callback)(frames, channels, sample_rate) else {
        return 0;
    }; // 221 行目: panic → プロセス abort
}
```

## 設計方針

以下は検討した実装案である。プロジェクトオーナーが `catch_unwind` 使用を許可しなかったため、採用していない。

`std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ...))` でコールバック呼び出しを囲み、panic 時はキャプチャ側は early return、再生側は 0 を返す。

### shiguredo-rust 規約との関係

shiguredo-rust スキルは「`std::panic::catch_unwind` を使わないこと。どうしても必要な場合は許可を得ること」と定めている。本 issue は FFI 境界でのユーザーコード panic という、ライブラリが制御不能な外部要因からプロセスを保護するケースであり、`catch_unwind` 以外に panic を止める手段がない。通常の Rust コード内のエラー処理に `catch_unwind` を使うのとは異なり、`extern "C"` 関数内では panic が C 側に伝播して UB / abort になるため、`catch_unwind` を外すとユーザーコードの panic がそのままプロセス全体の abort になる。

ただし、これが規約の「どうしても必要な場合」に該当するかはプロジェクトオーナーが判断することであり、**プロジェクトオーナーはこのケースに対する `catch_unwind` 使用を許可しなかった。** そのため本 issue はコード変更を行わずに closed にした。

## 完了条件

- ユーザークロージャの panic がプロセス abort につながらない
- キャプチャ側は panic 時に early return、再生側は 0 を返す
- panic を発生させるクロージャを渡して `frame_callback` / `playback_callback` が正常に return することを検証する単体テストを `src/capture_ffi.rs` と `src/playback_ffi.rs` の `#[cfg(test)]` モジュールに追加する（`std::panic::set_hook` でパニックメッセージを抑制する。`#[should_panic]` は catch するため使えない）
- `cargo clippy` / `cargo test` が通る

## 解決方法

プロジェクトオーナーが `std::panic::catch_unwind` の使用を許可しなかったため、コード変更は行わず closed にする。`extern "C"` 境界でユーザーコールバックの panic を止める手段は `catch_unwind` 以外にないため、panic 時にプロセスが abort する現在の挙動は変更しない。panic を契約として明文化する必要が生じた場合は別途扱う。

以下は検討した実装案であり、採用していない。

### 許可が得られた場合の実装案（不採用）

1. `capture_ffi.rs:218` の `(context.callback)(frame);` を `std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (context.callback)(frame)))` で囲み、`Err` 時は early return する
2. `playback_ffi.rs:221` の `(context.callback)(frames, channels, sample_rate)` を `catch_unwind` で囲む。戻り値が `Result<Option<PlaybackFrame>, _>` になるため、`let Ok(Some(frame)) = result else { return 0; };` のように統合する
3. 単体テストを追加する

## 後方互換

動作変更なし。コードを変更しないため、ユーザーコールバックの panic でプロセスが abort する挙動は変わらない。
