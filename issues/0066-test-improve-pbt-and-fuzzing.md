# PBT と fuzzing の品質を改善する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-pbt-and-fuzzing
- Polished: 2026-11-04

## 目的

PBT と fuzzing が検出できる不変条件を増やし、テスト対象と名前の不整合・不要なリーク・手動保守を解消する。

## 現状

- `pbt/tests/prop_error.rs` の `prop_null_pointer_contains_value` はケースごとに `Box::leak` して `'static str` を作っており、256 ケース分の文字列が解放されない。
- `SEED_ENV` と `CASES` が `pbt/tests/prop_common.rs` と `pbt/tests/prop_error.rs` に重複定義されている。
- `ERROR_SAMPLERS` は `Error` の全 variant を手動で列挙しており、variant を追加しても検出できない。`Error` は網羅 `match` で variant の追加をコンパイラに検査させられる。
- `prop_error_source_is_none` が「`source()` は常に None」を仕様として固定しており、`Error::DataTooLarge(TryFromIntError)` の内部エラーを `source()` で返す改善を妨げる。現行の `impl std::error::Error for Error` は `source()` を実装していないため、PBT の期待値を変更するだけではテストが失敗する。
- `from_s16` / `from_f32` の PBT は `frames` / `channels` / `data.len()` だけを検証し、`data` のバイト内容 (little-endian 格納) を検証していない。
- `as_s16` / `as_f32` がアラインメント不一致で `None` を返す分岐は、テストで `None` になることが確認されていない。PBT はアラインメントが合うケースだけを数え、合わない場合は何も assert しない。`AudioFrameOwned` の `data` は `Vec<u8>` であり、アロケータが返すポインタは `i16` / `f32` のアラインメントを満たすことが多いため、この分岐は `AudioFrameOwned` では確実に通せない。
- `fuzz/fuzz_targets/audio_frame_as_s16.rs` は `AudioFrameOwned` を対象にしており名前と一致しない。また 3 つの fuzz ターゲットはいずれも戻り値を捨てるだけで、`Ok` のときの長さ不変条件や `as_s16` / `as_f32` の排他性を検証していない。
- `write_playback_frame_to_buffer` は `pub(crate)` かつバックエンド feature 有効時のみコンパイルされるため、`default-features = false` の fuzz クレートから呼べない。fuzz は公開 API のみを対象にする規約に従い、この関数は `src/common.rs` の単体テストで検証する。fuzz は CI で実行されていない。

## 設計方針

- `Box::leak` をやめ、固定の文字列候補からサンプリングする。
- `SEED_ENV` / `CASES` は `pbt/tests/helpers.rs` に集約し、各 PBT ファイルから `mod helpers;` で参照する（`mod.rs` は使わない）。
- `ERROR_SAMPLERS` と網羅 `match` を併用し、variant の追加漏れをコンパイラで検出する。`Error` の variant を網羅する `match` で variant の index を返す関数を追加し、variant 追加時にコンパイルエラーになるようにする。`ERROR_SAMPLERS` の各要素が返す variant の index が `0..ERROR_SAMPLERS.len()` と一致することを到達ゲートで確認する。
- `src/error.rs` の `impl std::error::Error for Error` で `source()` を実装し、`DataTooLarge` だけ内部の `TryFromIntError` を返す。PBT を「`DataTooLarge` だけ `Some`、他は `None`」に変更する。
- `from_s16` / `from_f32` の LE バイト内容を検証する性質を追加する。`as_s16` / `as_f32` の未アラインメント時の `None` は、`AudioFrame` を `data: &bytes[1..]` のような 1 バイトずらしたスライスで構築して検証する。`AudioFrameOwned` の `Vec<u8>` はアロケータが返すポインタで、`i16` / `f32` のアラインメントを満たすことが多く、未アラインメント分岐を確実には通せない。`as_s16` / `as_f32` の排他性は、妥当なフレームに対する既存の `prop_mismatched_format_returns_none` で検証済みである。fuzz ターゲットでは任意のフレームに対して「両方が同時に `Some` にならないこと」を assert する。
- fuzz ターゲットに不変条件の assert を追加する。`audio_frame_as_s16` は `AudioFrameOwned` の `as_s16` / `as_f32` の両方を対象にしているため、`audio_frame_owned` にリネームし、`[[bin]]` の `name` と `path`、ファイル名を揃える。`playback_frame_from_s16` / `playback_frame_from_f32` は `Ok` のとき `frame.frames * frame.channels == data.len()` と `frame.data.len() == frame.frames * frame.channels * bytes_per_sample` を assert する。`write_playback_frame_to_buffer` は公開 API ではないため fuzz 対象にしない。
- CI の `schedule` イベントで実行するジョブを追加し、`rustup toolchain install nightly` と `cargo install cargo-fuzz` の後、`make fuzzing` を実行する。push では実行しない。

## 完了条件

- PBT に不要なリークと重複定義が無い。
- `Error` の variant 追加漏れをコンパイラが検出する。`ERROR_SAMPLERS` の各要素が返す variant の index が `0..ERROR_SAMPLERS.len()` と一致することを到達ゲートで確認する。
- `src/error.rs` の `Error::source()` が `DataTooLarge` の内部エラーを返し、他の variant は `None` を返す。PBT がそれを検証している。
- PBT が LE 格納とアラインメント分岐を検証している。
- fuzz ターゲットが不変条件を検証し、`audio_frame_owned` の名前が対象と一致している。CI の `schedule` で短時間の smoke fuzz が回る。
