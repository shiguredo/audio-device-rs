# PBT と fuzzing の品質を改善する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-pbt-and-fuzzing
- Polished: {YYYY-MM-DD}

## 目的

PBT と fuzzing が検出できる不変条件を増やし、テスト対象と名前の不整合・不要なリーク・手動保守を解消する。

## 現状

- `pbt/tests/prop_error.rs` の `prop_null_pointer_contains_value` はケースごとに `Box::leak` して `'static str` を作っており、256 ケース分の文字列が解放されない。
- `SEED_ENV` と `CASES` が `pbt/tests/prop_common.rs` と `pbt/tests/prop_error.rs` に重複定義されている。
- `ERROR_SAMPLERS` は `Error` の全 variant を手動で列挙しており、variant を追加しても検出できない。`Error` は網羅 `match` で variant 数の一致をコンパイラに検査させられる。
- `prop_error_source_is_none` が「`source()` は常に None」を仕様として固定しており、`Error::DataTooLarge(TryFromIntError)` の内部エラーを `source()` で返す改善を妨げる。
- `from_s16` / `from_f32` の PBT は `frames` / `channels` / `data.len()` だけを検証し、`data` のバイト内容 (little-endian 格納) を検証していない。
- `as_s16` / `as_f32` がアラインメント不一致で `None` を返す分岐は、テストで `None` になることが確認されていない。PBT はアラインメントが合うケースだけを数え、合わない場合は何も assert しない。
- `fuzz/fuzz_targets/audio_frame_as_s16.rs` は `AudioFrameOwned` を対象にしており名前と一致しない。また 3 つの fuzz ターゲットはいずれも戻り値を捨てるだけで、`Ok` のときの長さ不変条件や `as_s16` / `as_f32` の排他性を検証していない。
- `write_playback_frame_to_buffer` 用の fuzz ターゲットが無い。fuzz は CI で実行されていない。

## 設計方針

- `Box::leak` をやめ、固定の文字列候補からサンプリングする。
- `SEED_ENV` / `CASES` は `pbt/tests/helpers/` などに集約する。
- `ERROR_SAMPLERS` と網羅 `match` を併用し、variant の追加漏れをコンパイラで検出する。
- `source()` の PBT を「`DataTooLarge` だけ `Some`、他は `None`」に変更する。
- `from_s16` / `from_f32` の LE バイト内容、`as_s16` / `as_f32` の排他性と未アラインメント時の `None` を検証する性質を追加する。
- fuzz ターゲットに不変条件の assert を追加し、名前を実態に合わせるか `AudioFrame` を対象にする。`write_playback_frame_to_buffer` 用のターゲットを追加する。CI または定期実行で短時間の smoke fuzz を回す。

## 完了条件

- PBT に不要なリークと重複定義が無い。
- `Error` の variant 追加漏れをコンパイラが検出する。
- PBT が LE 格納とアラインメント分岐を検証している。
- fuzz ターゲットが不変条件を検証し、CI か定期実行で回る。
