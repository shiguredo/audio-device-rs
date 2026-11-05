# コールバック panic 時の契約を定義しプラットフォーム間で揃える

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/change-callback-panic-contract
- Polished: {YYYY-MM-DD}

## 目的

利用者コールバックの panic 時の挙動が未定義かつプラットフォーム間で非対称な状態を解消し、契約として明文化する。

## 現状

- macOS / Linux の `frame_callback` / `playback_callback` は `extern "C" fn` の中で利用者の `FnMut` を直接呼ぶため、panic するとプロセスが abort する。
- WASAPI ではワーカースレッド上で panic するとスレッドだけが終了し、`running` は真のまま残り、`stop()` の `let _ = handle.join()` が panic を捨てるため利用者は異常を観測できない。取得済みバッファが `ReleaseBuffer` されないままにもなる。
- `AudioCapture::new` / `AudioPlayback::new` の rustdoc には panic 禁止の契約も挙動の記載も無い。
- 過去に FFI 境界での `catch_unwind` 使用がプロジェクトオーナーに許可されず、コード変更を行わずに close された経緯がある。panic を契約として明文化する必要が生じた場合は別途扱うとされていた。

## 設計方針

- まず rustdoc に「コールバック内で panic すると macOS / Linux ではプロセスが abort し、Windows ではワーカースレッドが停止してセッションが無音になる」旨を明記する。
- プロジェクトオーナーの許可が得られる場合は `std::panic::catch_unwind(AssertUnwindSafe(...))` で FFI 境界を保護し、panic 時はキャプチャはフレーム破棄、再生は無音を返し、セッションを停止状態にする。
- WASAPI の `join()` の `Err` を無視しない扱いを決める。
- `CHANGES.md` に契約変更を記載する。

## 完了条件

- panic 時の挙動が rustdoc と `CHANGES.md` に明記されている。
- プラットフォーム間で観測される挙動が一致しているか、差異が明示されている。
- `cargo test --workspace` が通る。
