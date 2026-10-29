# 実機の再生タイミングテストを安定させる

- Created: 2026-10-27
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-macos-playback-timing-flake
- Polished: {YYYY-MM-DD}

## 目的

macOS CI で `tests/test_device_timing.rs` の `playback_callbacks_track_audio_time` が不安定に失敗するのを防ぐ。
許容値を広げたりリトライしたりせず、測定方法とテスト実行環境を修正して、再生レートの検証精度を維持する。

## 現状

`tests/test_device_timing.rs` には、デフォルト入力デバイスを使う `capture_timestamps_track_audio_time` と、デフォルト出力デバイスを使う `playback_callbacks_track_audio_time` がある。
`cargo test --workspace` では同じテストバイナリ内の 2 テストが並列に実行される。

`playback_callbacks_track_audio_time` は `PlaybackCall` にコールバック呼び出し時刻 `at` と要求フレーム数を記録し、`verify_playback_calls` で次のように検証している。

- `PLAYBACK_WARMUP` 以降のコールバックのうち、最初と最後の `at` の差を経過時間とする
- 最後のコールバックを除いた要求フレーム数の合計を、経過時間 × サンプルレートと比較する
- 差が `PLAYBACK_TIMING_TOLERANCE` (50 ms) 以内であることを確認する

macOS CI で次の失敗が発生した。

- 経過時間: 2.013 秒
- 要求フレーム: 99,840
- 期待フレーム: 96,621
- 差: 3,219 フレーム（48 kHz で約 67 ms）
- 許容: 2,400 フレーム（50 ms）

失敗したジョブ: [CI (macOS) / run 40464045021](https://github.com/shiguredo/audio-device-rs/actions/runs/40464045021/job/122055098535)

同じコードの PR #22 の macOS CI は成功している。
また、`Device Test (macOS)` は `capture_timestamps_track_audio_time` を `--skip` しているため再生テストだけが動き、こちらも成功している。
`CI (macOS)` のログでは 2 テストが同時に開始し、再生テストの測定区間の間もキャプチャテストが動作していた。

## 原因

### 端点 2 点だけを使った測定

`PlaybackCall::at` は CoreAudio のコールバックスレッドが実行された時刻であり、音声が実際に再生された時刻ではない。
測定区間の先頭または末尾で 1 回でも 50 ms を超えるスケジューリング遅延が起きると、経過時間だけが短くなり、要求フレーム数は変わらないため、誤差が許容値を超える。

今回の 99,840 フレームは 10 ms バッファ 208 本分（2.08 秒）に相当する。
測定された経過時間 2.013 秒との差は約 67 ms であり、測定区間の境界で 1 回大きな遅延が起きた場合の値と整合する。

### キャプチャと再生の同時実行

`cargo test` は同じテストバイナリ内のテストを並列に実行する。
`capture_timestamps_track_audio_time` は 5 秒間の入力、`playback_callbacks_track_audio_time` は 3 秒間の出力を行うため、両者の実行区間が重なる。

macOS のホステッドランナー上で入出力が同時に動くと、再生コールバックのスレッドが遅延しやすくなる。
`capture_timestamps_track_audio_time` を実行しない `Device Test (macOS)` では再生テストが成功していることも、この干渉を裏付けている。

## 設計方針

### 全測定点を使ったロバスト推定

`PLAYBACK_WARMUP` 以降のすべての `(at, 累積要求フレーム数)` の組について傾きを求め、その中央値 (Theil-Sen 推定) をフレームレートとして検証する。

- 許容値は 50 ms のまま変更しない
- 2 秒間で 2.5% を超える平均レート誤差を検出する精度を維持する
- 単発のスケジューリング遅延が結果に与える影響を、測定区間内の位置によらず抑える
- 推定処理を純粋関数として切り出し、合成データで単体テストする
- 2 点では端点 2 点の傾きに縮退するため、測定区間 1.8 秒以上、測定点 10 点以上を要求する

### タイミングテストの直列化

`capture_timestamps_track_audio_time` と `playback_callbacks_track_audio_time` が同時に動かないようにする。
テスト用の `static Mutex<()>` で直列化し、共有状態の保護ではなくテスト実行の排他制御であることと、入出力の同時実行によるコールバック遅延を避ける目的をコメントに書く。

CI の macOS ジョブだけで `--test-threads=1` を指定する方法もあるが、ローカルの `cargo test` でも再発しないよう、テスト側で直列化する。

## 完了条件

- `playback_callbacks_track_audio_time` が端点 2 点ではなく、全測定点から推定したフレームレートで検証すること
- `PLAYBACK_TIMING_TOLERANCE` を広げず、リトライも追加しないこと
- 測定区間長と測定点数の下限を設けること
- `capture_timestamps_track_audio_time` と `playback_callbacks_track_audio_time` が同時に実行されないこと
- レート推定関数に、末尾と中央に遅延を加えた合成データ、意図的に 2 倍のレートにした合成データ、測定点数と測定区間長が不足する場合の単体テストがあること
- macOS CI の `cargo test --workspace` が成功し、失敗ジョブの再実行でも再発しないこと
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` が通ること

## テスト

- レート推定関数を直接呼び、一定レートの末尾または中央に遅延を加えても許容範囲に収まることを確認する
- レート推定関数を直接呼び、2 倍のレートの合成データでは許容範囲を超えることを確認する
- 測定区間長または測定点数が下限未満の場合は、再生レートの検証に不足すると判定することを確認する
- macOS の実機で `playback_callbacks_track_audio_time` を実行し、50 ms の許容値で成功することを確認する
- `capture_timestamps_track_audio_time` と同時に実行しても失敗しないことを確認する

## 参考

- [CI (macOS) / run 40464045021](https://github.com/shiguredo/audio-device-rs/actions/runs/40464045021/job/122055098535)
- PR #22
- `Device Test (macOS)` の成功ログ
