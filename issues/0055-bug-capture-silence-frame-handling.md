# キャプチャの無音・データ穴の扱いをバックエンド間で揃える

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-capture-silence-frame-handling
- Polished: {YYYY-MM-DD}

## 目的

キャプチャ時に無音やデータ穴が発生した場合の挙動をバックエンド間で統一し、フレームが届かないことによるタイムスタンプの飛びとテストの不安定化を解消する。

## 現状

- WASAPI キャプチャは `AUDCLNT_BUFFERFLAGS_SILENT` のパケットを `ReleaseBuffer` して `continue` するため、ユーザーコールバックが呼ばれない。
- PulseAudio キャプチャは `pa_stream_peek` の穴 (`data == NULL`, `length > 0`) を読み飛ばし、コールバックを呼ばずに `read_bytes` だけを加算する。
- CoreAudio は無音を含むデータをそのままコールバックへ渡す。バックエンド間で「無音時にフレームが届くか」が異なる。
- `capture.rs` の doc は「キャプチャしたフレームごとに呼ばれる」としか書いておらず、無音を落とす仕様が書かれていない。
- `tests/test_device_timing.rs` の `verify_capture_timestamps` は連続フレームのタイムスタンプ差が直前フレームの音声時間と 1 ms 以内であることを全ペアで要求するため、無音パケットや穴が 1 つ発生するだけで失敗し得る。Windows のミュート中などは SILENT が通常発生する。

## 設計方針

- 無音パケットと穴もゼロ埋めした `AudioFrame` としてコールバックへ渡し、バックエンド間で挙動を揃える。
- 落とす仕様を維持する場合は、rustdoc と `CHANGES.md` に「無音・穴のフレームは渡されず、タイムスタンプが飛ぶ」と明記する。
- `verify_capture_timestamps` の検証を「スキップが無いことを確認できた区間」に限定するか、「タイムスタンプ差が音声時間以上で、上限付き」に変更する。

## 完了条件

- 無音・穴の扱いがバックエンド間で同じになるか、仕様として明文化されている。
- 実機テストがミュート中やデータ穴発生時に失敗しない。
- 無音・穴のケースを検証するテストが追加されている。
- `cargo test --workspace` が通る。
