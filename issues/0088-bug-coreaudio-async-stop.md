# CoreAudio が AudioQueue の非同期停止を検出できない

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-coreaudio-async-stop
- Polished: {YYYY-MM-DD}

## 目的

CoreAudio はデバイス取り外しやハードウェア再構成で `AudioQueue` が自発的に
停止しても、Rust 側の `running` フラグを実行中のまま保持する。その後に
`start` を呼んでも no-op で成功を返し、コールバックが来ない状態から復帰できない。
キューが実際に動作しているかを検出し、再開始またはエラー通知できるようにする。

## 現状

`audio_coreaudio_session_start` / `audio_coreaudio_playback_session_start` は
`atomic_load(&session->running)` が真なら 0 を返す。`audio_coreaudio.m` には
`AudioQueueAddPropertyListener` と `kAudioQueueProperty_IsRunning` の参照がなく、
キューの実際の状態を取得する箇所もない。

macOS SDK の `AudioQueue.h` は `kAudioQueueProperty_IsRunning` について
「オーディオデバイスが開始・停止したときに通知が送られる。これは start / stop
関数が呼ばれたときとは限らない」と定義している。デバイス取り外しなどで
キューが停止しても `running` は 1 のまま残る。

利用者が再度 `start` を呼ぶと `running` が真なので 0 (成功) を返すが、
`AudioQueueStart` は呼ばれず、コールバックも来ない。`stop` を呼べば内部状態は
リセットされるが、その必要があることを利用者が知る手段がない。

0056 は start 失敗後のバッファ回収を扱っている。本 issue は、start / stop の
呼び出しとは独立に起きるキューの停止を検出する部分を扱う。

## 設計方針

- `AudioSession` / `PlaybackSession` に `kAudioQueueProperty_IsRunning` を
  監視する property listener を登録する。listener はキューの実際の実行状態を
  atomic なフィールドへ反映する
- `AudioQueue` の停止を検出したら `running` を 0 にする。enqueue 済みの
  バッファが残っている可能性があるため、`AudioQueueReset` で回収してから
  次の `start` で enqueue できる状態にする
- `start` はローカルの `running` だけで判断せず、必要に応じて
  `AudioQueueGetProperty(kAudioQueueProperty_IsRunning)` で実際の状態を
  確認する。実際には停止しているのに `running` が真なら、状態を戻して
  再 enqueue / `AudioQueueStart` を行う
- property listener は `audio_coreaudio_session_destroy` /
  `audio_coreaudio_playback_session_destroy` で解除する。
  解除前にキューを dispose しない
- 非同期停止を利用者へ通知する API を追加するか、`start` で再開始するかを
  決める。通知しない場合は rustdoc に「デバイス取り外し後は `stop` して
  `start` し直す必要がある」と明記する
- 0056 の start 失敗時の `AudioQueueReset` と同じ順序を使い、二重 enqueue を
  起こさない

## 完了条件

- 外部要因で `AudioQueue` が停止した後、`kAudioQueueProperty_IsRunning` の
  監視によって `running` が実際の状態と一致する
- その状態で `start` を呼ぶと、再開始に成功するか、失敗を表すエラーを返す。
  成功の no-op を返してコールバックが来ない状態を残さない
- `stop` / `destroy` で property listener が解除され、解除後にキューが
  dispose される
- デバイス取り外しを再現できる macOS 実機テスト、またはコードレビューで
  確認する。再現できない環境では、`kAudioQueueProperty_IsRunning` の
  listener と `start` の状態確認をコードレビューで確認する
