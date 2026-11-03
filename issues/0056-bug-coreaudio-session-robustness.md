# CoreAudio のセッション開始失敗とバッファ長の不整合を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-coreaudio-session-robustness
- Polished: 2026-11-03

## 目的

CoreAudio でセッション開始に失敗した後に再開始できなくなる問題と、サンプルレートによってバッファ長がフレーム境界に揃わない問題を修正する。あわせて、ビット深度の判定が S16 / F32 以外を誤って扱う潜在的な問題を、将来の 24 bit などの変更に備えて防御的に修正する。

## 現状

- `audio_coreaudio_session_start` と `audio_coreaudio_playback_session_start` は `running = 1` の後に 3 本のバッファを enqueue し、途中の enqueue 失敗または `AudioQueueStart` 失敗時に `running = 0` にして `-1` を返すだけで、enqueue 済みバッファをキューから回収しない。
- macOS SDK の `AudioQueue.h` は `kAudioQueueErr_BufferEnqueuedTwice` を「A buffer was enqueued twice on an input queue (before being returned as a result of being filled or from Reset).」と定義している。`kAudioQueueErr_CannotStartYet` (ハードウェア再構成中。再試行推奨) や `kAudioQueueErr_Permissions` (マイク権限拒否など) で現実に失敗し得るため、キャプチャでは失敗後の再 `start` が同じバッファを再 enqueue して `kAudioQueueErr_BufferEnqueuedTwice` で恒久的に失敗する。再生側も、`AudioQueueStart` 失敗後にバッファが enqueue されたままになるため、同じバッファの再 enqueue で失敗し得る。
- `audio_coreaudio_session_create` と `audio_coreaudio_playback_session_create` は `bufferByteSize = mSampleRate * mBytesPerFrame / 100` をそのまま確保し、再生側は `mAudioDataByteSize` にも設定している。22050 Hz ステレオ S16 では 882 バイト (1 フレーム 4 バイトの倍数でない) になり、再生コールバックは 220 フレームだけ Rust に書かせたうえで 882 バイトを enqueue する。末尾 2 バイトはフレームの一部として余り、1 フレームに満たない端数が enqueue されるため、フレーム境界の不整合やグリッチの原因になる。
- `audio_input_callback` と `audio_output_callback` は `mBitsPerChannel == 16` 以外を一律 `AUDIO_FORMAT_F32` と判定している。現状は S16 固定なので到達しないが、24 bit などを扱う変更をすると Rust 側は 1 サンプル 4 バイトとして長さを計算し、バッファ外参照になる。また、32 bit 整数 (S32) は 32 bit 浮動小数点 (F32) と区別されず、F32 として扱われる。

## 設計方針

- start 失敗時は、`running` を 0 にしてから `AudioQueueReset(queue)` を呼び、enqueue 済みバッファをキューから回収してから `-1` を返す。`running` を先に 0 にするのは、reset 中に呼ばれるコールバックがバッファを再 enqueue しないようにするためである。`AudioQueueStop(queue, true)` でもバッファは回収できるが、start に失敗した停止状態のキューでは `AudioQueueReset` を使う。
- `bufferByteSize` を `mBytesPerFrame` の倍数に切り下げる。切り下げの結果が 0 になる場合は、少なくとも 1 フレーム分 (`mBytesPerFrame`) を確保する。再生側は enqueue 時に `mAudioDataByteSize = frames * mBytesPerFrame` を設定する。キャプチャ側は `AudioQueueAllocateBuffer` に渡すサイズをフレーム境界に揃える (入力キューは録音時に `mAudioDataByteSize` を設定するため、enqueue 時の設定は不要)。
- ビット深度と `mFormatFlags` を組み合わせて形式を判定する。`mFormatID == kAudioFormatLinearPCM` かつ `mBitsPerChannel == 16` かつ `mFormatFlags` に `kAudioFormatFlagIsSignedInteger` が立っている場合は `AUDIO_FORMAT_S16`、`mFormatID == kAudioFormatLinearPCM` かつ `mBitsPerChannel == 32` かつ `mFormatFlags` に `kAudioFormatFlagIsFloat` が立っている場合は `AUDIO_FORMAT_F32` とする。それ以外は `create` 時に `NULL` を返し、`start` させない。Rust 側へビット深度を渡す拡張は本 issue では行わない。
- `audio.h` のセッション API の共通仕様に、start 失敗後の再 start と、対応する音声形式 (S16 / F32 のみ) を明記する。`AudioCapture::start` / `AudioPlayback::start` の rustdoc にも、start 失敗後も同じセッションで再 start できることを明記する。`CHANGES.md` の `## develop` に FIX エントリを追加する。
- `tests/session_lifecycle.rs` に start 失敗後の再 start を検証するテストを追加する。start 失敗を確実に再現できない環境では、`start` が `Err` を返した場合に同じセッションで再度 `start` を呼び、2 回目の `start` が成功することを確認する。失敗が続く環境ではテストをスキップする。キャプチャは macOS CI でスキップされるため、再生側のテストを必須とし、キャプチャ側はコードレビューと実機確認で補う。`tests/test_device_timing.rs` に 22050 Hz の再生テストを追加し、`start` とコールバックが継続することを確認する。22050 Hz を受け付けない環境では `AudioPlayback::new` の失敗でテストをスキップする。バッファ長の計算とビット深度の判定は公開 API から直接観測できないため、コードレビューで確認する。

## 完了条件

- start 失敗後に同じセッションで再 start できる。
- 任意のサンプルレートでバッファ長が `mBytesPerFrame` の倍数になり、0 にならない。
- ビット深度と `mFormatFlags` の判定が S16 / F32 以外で誤動作せず、S16 / F32 以外の形式は `create` で拒否される。
- start 失敗後の再 start と、10 ms 分のバッファ長がフレーム境界に揃わないサンプルレート (例: 22050 Hz) のテストが追加されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が、CI の全構成 (PulseAudio / PipeWire / CoreAudio / WASAPI) で通る。
