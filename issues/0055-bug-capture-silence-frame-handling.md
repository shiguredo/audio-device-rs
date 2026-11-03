# キャプチャの無音・データ穴の扱いをバックエンド間で揃える

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-capture-silence-frame-handling
- Polished: 2026-11-03

## 目的

キャプチャ時に無音やデータ穴が発生した場合の挙動をバックエンド間で統一し、フレームが届かないことによるタイムスタンプの飛びとテストの不安定化を解消する。

## 現状

- WASAPI キャプチャは `AUDCLNT_BUFFERFLAGS_SILENT` のパケットを `ReleaseBuffer` して
  `continue` するため、ユーザーコールバックが呼ばれない。Microsoft の
  [`_AUDCLNT_BUFFERFLAGS`](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/ne-audioclient-_audclnt_bufferflags)
  は SILENT を「パケットの全データを無音として扱い、実際のデータ値を無視する」と定義している。
- PulseAudio キャプチャは `pa_stream_peek` の穴 (`data == NULL`, `length > 0`) を読み飛ばし、コールバックを呼ばずに `read_bytes` だけを加算する。
- CoreAudio は無音を含むデータをそのままコールバックへ渡す。PipeWire も `chunk->size > 0` の間はデータをそのまま渡すため、無音はゼロデータとして届く。WASAPI と PulseAudio だけが無音・穴のフレームを落としている。
- `capture.rs` の doc は「キャプチャしたフレームごとに呼ばれる」と書いているが、無音・穴の扱いが書かれていない。`common.rs` の `AudioFrame` と `audio.h` のコールバック共通仕様にも、無音・穴の扱いが書かれていない。
- `tests/test_device_timing.rs` の `verify_capture_timestamps` は連続フレームのタイムスタンプ差が直前フレームの音声時間と 1 ms 以内であることを全ペアで要求するため、無音パケットや穴が 1 つ発生するだけで失敗し得る。Windows のミュート中などに SILENT が発生し得る。

## 設計方針

- 無音パケットと穴もゼロ埋めした `AudioFrame` としてコールバックへ渡し、バックエンド間で挙動を揃える。WASAPI の SILENT パケットと PulseAudio の穴を対象にし、CoreAudio と PipeWire は現状のゼロデータを渡す動作を維持する。
- ゼロ埋めしたフレームは、元のパケットまたは穴の位置に対応するフレーム数・チャンネル数・
  サンプルレート・タイムスタンプを持つ。WASAPI の SILENT パケットでは
  `AUDCLNT_BUFFERFLAGS_SILENT` を無音として扱い、`data_ptr` が NULL でもゼロ埋めする。
  PulseAudio の穴では `length` から求めたフレーム数をゼロ埋めし、フレーム境界に満たない
  端数は切り捨てる。
- ゼロ埋めして渡す仕様を `capture.rs` と `common.rs` の rustdoc、`audio.h` のコールバック共通仕様に明記する。`CHANGES.md` にはキャプチャの無音・穴をゼロ埋めして渡すようにする FIX エントリを追加する。
- `issues/closed/0044-bug-wasapi-capture-timestamp.md` では `AUDCLNT_BUFFERFLAGS_SILENT` の扱いを変更しないとしていたが、本 issue ではバックエンド間の挙動を揃えるため、ゼロ埋めして渡す方針に変更する。
- `verify_capture_timestamps` は、ゼロ埋めしたフレームが届く前提で、連続フレームのタイムスタンプ差が直前フレームの音声時間と 1 ms 以内で一致することを検証する。

## 完了条件

- 無音・穴の区間がゼロ埋めしたフレームとしてコールバックへ渡され、バックエンド間で同じ扱いになる。
- `capture.rs` と `common.rs` の rustdoc、`audio.h` のコールバック共通仕様に、無音・穴をゼロ埋めして渡すことが書かれている。
- `CHANGES.md` に FIX エントリが追加されている。
- 実機テストがミュート中やデータ穴発生時に失敗しない。
- 無音・穴のケースでゼロ埋めしたフレームが渡り、タイムスタンプ差が直前フレームの音声時間と 1 ms 以内で一致することを検証するテストが追加されている。
- `cargo test --workspace` が通る。
