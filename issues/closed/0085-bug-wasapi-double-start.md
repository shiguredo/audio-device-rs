# WASAPI の start が実行中を検査せず mem::forget で二重起動できる

- Created: 2026-11-25
- Completed: 2026-11-25
- Branch: feature/fix-wasapi-double-start
- Polished: {YYYY-MM-DD}

## 目的

`AudioCaptureSession` / `AudioPlaybackSession` を `std::mem::forget` した後に
再度 `start` を呼ぶと、WASAPI のワーカースレッドが 2 本起動し、同じ
`IAudioCaptureClient` / `IAudioRenderClient` と `RefCell` を共有して
未定義動作に到達する。safe なコードから到達できるメモリー安全性の欠陥を塞ぐ。

## 現状

`WasapiCaptureImpl::start` は `self.session` が `Some` かどうかだけを確認し、
`self.running` と `self.capture_thread` を確認せずに `IAudioClient::Start` と
スレッド生成を行う。`WasapiPlaybackImpl::start` も同じ構造である。

`AudioCaptureSession` は `&mut AudioCapture` を借用するが、`std::mem::forget` は
safe であり、借用を終わらせたまま `Drop` を実行しない。そのため次のコードが
コンパイルを通る。

```rust
let session = capture.start()?;
std::mem::forget(session);
let session2 = capture.start()?;
```

`IAudioClient::Start` は既に開始済みの場合に `S_FALSE` を返す。`windows-rs` の
`map_err` は負の HRESULT だけをエラーにするため、2 回目の `Start` は成功扱いに
なる。`self.capture_thread = Some(handle)` は既存の `JoinHandle` を drop するため、
1 本目のスレッドは detach される。

`WasapiCaptureImpl::stop` / `WasapiPlaybackImpl::stop` は `running` が真のときだけ
動き、保持している `JoinHandle` を 1 本だけ join する。`Drop` はその後に
イベントハンドルを閉じ、`Box<WasapiCaptureContext>` /
`Box<WasapiPlaybackContext>` を解放する。detach されたスレッドがまだ
ユーザーコールバックを実行している場合、コンテキストとコールバックの
解放後使用になる。閉じたイベントハンドルに対する `WaitForSingleObject` は
`WAIT_FAILED` を返し、現在の実装では `continue` でビジーループになる。

`AudioCapture::start` の rustdoc にある `compile_fail` doctest は、ガードを
保持したままの再 `start` をコンパイルエラーにできることだけを示しており、
`mem::forget` による実行時の二重起動は防げない。

## 設計方針

- `WasapiCaptureImpl::start` / `WasapiPlaybackImpl::start` の先頭で、
  `running` が真かつ `capture_thread` / `playback_thread` が `Some` なら
  `Ok(())` を返して `IAudioClient::Start` もスレッド生成も行わない
- `running` が偽でハンドルが `Some` の場合は、終了済みのスレッドを join して
  ハンドルを clear してから開始する。終了済みハンドルを残したまま新しい
  スレッドを起動しない
- `stop` は `running` フラグではなくスレッドハンドルの有無で join する。
  `running` が既に false でも、ハンドルが残っていれば join してから
  イベントハンドルとコンテキストを解放する
- `Drop` は `stop` の後にイベントハンドルを閉じ、その後にコンテキストを
  解放する順序を守る。`stop` が join を完了できない異常系でも、コンテキストを
  先に解放しない
- `AudioCaptureSession` / `AudioPlaybackSession` の rustdoc に、
  `mem::forget` された場合でも `start` は実行中なら no-op になることを明記する
- Windows の単体テストで、`mem::forget` の後に再度 `start` してもワーカーが
  1 本のままで、`stop` がその 1 本を join することを確認する。テスト用の
  モックは使わず、実際の WASAPI セッションで確認できる範囲に限定する

## 完了条件

- `mem::forget` の後に `start` を呼んでも、キャプチャと再生のワーカーが
  2 本にならない
- ワーカーがエラーで終了した後に `start` を呼ぶと、終了済みのハンドルを
  join してから新しいワーカーを 1 本だけ起動する
- `stop` / `Drop` が、実行中のワーカーを join してからイベントハンドルと
  コンテキストを解放する
- `mem::forget` を使った再 `start` のテストが追加されている。
  Windows 実機で実行できない環境では、コードレビューと Windows ターゲットの
  `cargo clippy` で確認する
- `cargo clippy --target x86_64-pc-windows-msvc` が通る

## 解決方法

closed にする。コード変更は行わない。

`IAudioClient::Start` は、既に開始済みのストリームに対して `S_FALSE` ではなく
`AUDCLNT_E_NOT_STOPPED` を返す。Microsoft のドキュメントにこの戻り値があり、
Wine の WASAPI テストでも二重 `Start` が `AUDCLNT_E_NOT_STOPPED` になることを
確認している
([IAudioClient::Start](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudioclient-start)、
[render.c](https://github.com/wine-mirror/wine/blob/master/dlls/mmdevapi/tests/render.c#L502)、
[capture.c](https://github.com/wine-mirror/wine/blob/master/dlls/mmdevapi/tests/capture.c#L357))。

`AUDCLNT_E_NOT_STOPPED` は `0x88890005` の負の HRESULT であるため、
`windows-rs` の `Start()` は `HRESULT::ok()` により `Err` を返す。
`WasapiCaptureImpl::start` と `WasapiPlaybackImpl::start` は
`Start().map_err(|_| Error::SessionStartFailed)?` でこのエラーを伝播するため、
`running` の更新、ワーカースレッドの生成、`JoinHandle` の上書きには到達しない。
したがって、二重起動、ワーカーの detach、`RefCell` の共有、
コンテキストの解放後使用、`WaitForSingleObject` のビジーループはいずれも
発生しない。

`S_FALSE` は `IAudioClient::Stop` が「既に停止済み」の場合に返す値であり、
`Start` の戻り値ではない。報告は `Start` と `Stop` の戻り値を混同している。

`compile_fail` doctest が `mem::forget` を防げないという指摘は正しいが、
実行時には上記のとおり `Start` のエラーで戻るため、未定義動作にはならない。

Windows 実機でも確認した。Windows 11 25H2 (ビルド 26200.9974) の既定の
再生デバイスと既定のキャプチャデバイスに対して、ライブラリと同じ初期化条件
(共有モード、イベント駆動、形式変換あり) で vtable を直接呼び出し、生の
HRESULT を取得した結果は次のとおり。

- 再生・キャプチャとも、2 回目の `Start` は `AUDCLNT_E_NOT_STOPPED` (0x88890005)
- 1 回目の `Stop` は `S_OK`、2 回目の `Stop` は `S_FALSE` (0x00000001)
- `Stop` 後の `Start` は `S_OK`、その直後の `Start` は再び
  `AUDCLNT_E_NOT_STOPPED`
- 公開 API を `std::mem::forget` でだました後の再 `start` は、キャプチャと
  再生のどちらも `Error::SessionStartFailed` になる

将来 `Start` が成功する状態に変わった場合に備えて `running` を Rust 側で
確認する防御は考えられるが、この issue の未定義動作を修正するものではない。
