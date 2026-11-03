# コールバック panic 時の契約を定義する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/change-callback-panic-contract
- Polished: 2026-11-04

## 目的

利用者コールバックの panic 時の挙動が未定義かつプラットフォーム間で非対称な状態を、最終的な挙動を契約として rustdoc と `CHANGES.md` に明記する。プラットフォーム間の挙動を揃えることは目標とするが、実現できない場合は差異を明示する。

## 現状

- macOS / Linux の `frame_callback` / `playback_callback` は `extern "C" fn` の中で利用者の `FnMut` を直接呼ぶため、panic するとプロセスが abort する。
- WASAPI ではワーカースレッド上で panic するとスレッドだけが終了し、`running` は真のまま残り、`stop()` の `let _ = handle.join()` が panic を捨てるため利用者は異常を観測できない。取得済みバッファが `ReleaseBuffer` されないままにもなる。キャプチャでは以後のコールバックが呼ばれず、再生では無音になる。
- `AudioCapture::new*` / `AudioPlayback::new*` の rustdoc には panic 禁止の契約も挙動の記載も無い。
- 過去に FFI 境界での `catch_unwind` 使用がプロジェクトオーナーに許可されず、コード変更を行わずに closed にした経緯がある (`issues/closed/0029-bug-extern-c-callback-panic.md`)。panic を契約として明文化する必要が生じた場合は別途扱うとされていた。

## 設計方針

- `catch_unwind` の採否を先に決める。プロジェクトオーナーの許可が得られる場合は、macOS / Linux の `frame_callback` / `playback_callback` を `std::panic::catch_unwind(std::panic::AssertUnwindSafe(...))` で保護する。許可が得られない場合はコード変更を行わず、現状の挙動を契約として明記する。WASAPI のワーカースレッドは FFI 境界ではないため、`catch_unwind` を追加しない。
- `catch_unwind` を入れる場合、panic を捕捉したらキャプチャはフレームを破棄し、再生は無音を返す。C API はコールバックのスレッド上からの `stop` を禁止しているため、コールバック内では `stop` を呼ばず、panic を捕捉したことを示すフラグを立てて以後のユーザーコールバック呼び出しを止める。実際のセッション停止は利用者が別スレッドから `AudioCaptureSession::stop` / `AudioPlaybackSession::stop` を呼ぶまで行わない。フラグは `start()` でリセットする。
- WASAPI の `join()` の `Err` は panic の発生を示す。`stop()` の戻り値は `()` のまま、`Err` を検出してセッションを停止済みとして扱い、rustdoc に「コールバックの panic は `stop()` では報告されない」と明記する。
- rustdoc は `AudioCapture::new*` / `AudioPlayback::new*` に最終的な panic 時の挙動を書き、`AudioCaptureSession::stop` / `AudioPlaybackSession::stop` には panic が報告されないことを書く。`issues/0072-doc-public-api-contracts.md` と重複する panic の記述は、本 issue ではコールバック panic に限定する。
- `CHANGES.md` の `## develop` に契約を記載する。`catch_unwind` を入れて挙動を変える場合は `[CHANGE]`、rustdoc のみで挙動を変えない場合は `[UPDATE]` とする。

## 完了条件

- `catch_unwind` の採否が決まり、最終的な panic 時の挙動が rustdoc と `CHANGES.md` に明記されている。
- rustdoc には、採否に応じた最終的な挙動が書かれている。`catch_unwind` を入れない場合は macOS / Linux の abort と Windows のワーカースレッド停止 (キャプチャは以後のコールバックが呼ばれない、再生は無音)、入れる場合は macOS / Linux の捕捉後の挙動と Windows のワーカースレッド停止が対象である。
- `catch_unwind` を入れる場合、panic するコールバックを渡して `frame_callback` / `playback_callback` が panic せずに戻ることを単体テストで確認する。
- `cargo test --workspace` と `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る。
- プラットフォーム間で観測される挙動が一致しているか、差異が明示されている。
