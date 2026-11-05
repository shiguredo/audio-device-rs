# CoreAudio のセッション開始失敗とバッファ長の不整合を修正する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-coreaudio-session-robustness
- Polished: {YYYY-MM-DD}

## 目的

CoreAudio でセッション開始に失敗した後に再開始できなくなる問題と、サンプルレートによってバッファ長がフレーム境界に揃わない問題を修正する。

## 現状

- `audio_coreaudio_session_start` と `audio_coreaudio_playback_session_start` は `running = 1` の後に 3 本のバッファを enqueue し、途中の enqueue 失敗または `AudioQueueStart` 失敗時に `running = 0` にして `-1` を返すだけで、enqueue 済みバッファをキューから回収しない。
- macOS SDK の `AudioQueue.h` は `kAudioQueueErr_BufferEnqueuedTwice` を「入力キューで、バッファが埋まるか Reset で返却される前に二度 enqueue された」と定義している。`kAudioQueueErr_CannotStartYet` (ハードウェア再構成中。再試行推奨) や `kAudioQueueErr_Permissions` (マイク権限拒否) で現実に失敗し得るため、失敗後の再 `start` は同じバッファを再 enqueue して恒久的に失敗する。
- `audio_coreaudio_session_create` と `audio_coreaudio_playback_session_create` は `bufferByteSize = mSampleRate * mBytesPerFrame / 100` をそのまま確保し、再生側は `mAudioDataByteSize` にも設定している。22050 Hz ステレオ S16 では 882 バイト (1 フレーム 4 バイトの倍数でない) になり、再生コールバックは 220 フレームだけ Rust に書かせたうえで 882 バイトを enqueue する。末尾 2 バイトは前回の内容が残り、グリッチやエラーの原因になる。
- `audio_input_callback` と `audio_output_callback` は `mBitsPerChannel == 16` 以外を一律 `AUDIO_FORMAT_F32` と判定している。現状は S16 固定なので到達しないが、24 bit などを扱う変更をすると Rust 側は 1 サンプル 4 バイトとして長さを計算し、バッファ外参照になる。

## 設計方針

- start 失敗時に `AudioQueueStop(queue, true)` または `AudioQueueReset(queue)` でバッファを回収してから戻す (または失敗時にキューを作り直す)。
- `bufferByteSize` を `mBytesPerFrame` の倍数に切り下げ、enqueue 時に `frames * mBytesPerFrame` を `mAudioDataByteSize` に設定する。
- ビット深度は明示的に判定し、16 / 32 以外はコールバックを呼ばずにエラーにするか、Rust 側へビット深度を渡す。

## 完了条件

- start 失敗後に同じセッションで再 start できる。
- 任意のサンプルレートでバッファ長がフレーム境界に揃う。
- ビット深度の判定が S16 / F32 以外で誤動作しない。
- 失敗後の再 start と非整数レートのテストが追加されている。
- `cargo test --workspace` が通る。
