# PlaybackFrame のフィールドを非公開にして不変条件を強制する

- Created: 2026-11-03
- Completed: {YYYY-MM-DD}
- Branch: feature/change-playback-frame-private-fields
- Polished: {YYYY-MM-DD}

## 目的

`PlaybackFrame` のフィールドがすべて public であり、構造体リテラルで `data` / `frames` / `channels` / `sample_rate` / `format` の不整合を作れる。フィールドを非公開にして `from_s16` / `from_f32` だけから構築させ、メタデータの不整合を型で防ぐ。

## 現状

- `src/common.rs` の `PlaybackFrame` は `data` / `frames` / `channels` / `sample_rate` / `format` をすべて `pub` で公開している。
- `PlaybackFrame::from_s16` / `from_f32` は `channels > 0` と `data.len() % channels == 0` を検証し、`frames` を `data` のサンプル数から計算する。構造体リテラルで直接構築すると、この不変条件を破れる。
- 再生経路は `frame.format` を変換元として使い、`frame.data` の実際の形式が `frame.format` と異なる場合は検出できない。`src/playback_ffi.rs` / `src/playback_wasapi.rs` は `frame.data` / `frame.format` を直接参照している。
- `pbt/tests/prop_common.rs` は `frame.data` / `frame.frames` / `frame.channels` / `frame.sample_rate` / `frame.format` を直接読んでいる。

## 設計方針

- `PlaybackFrame` のフィールドを private にし、`data()` (`&[u8]`)、`frames()` (`i32`)、`channels()` (`i32`)、`sample_rate()` (`i32`)、`format()` (`AudioFormat`) のアクセサを追加する。
- `from_s16` / `from_f32` を唯一の構築経路にする。`PlaybackFrame` の doc に、フィールドの意味と `from_s16` / `from_f32` が保証する整合条件を書く。
- `src/playback_ffi.rs` / `src/playback_wasapi.rs` と `pbt/tests/prop_common.rs` の `PlaybackFrame` のフィールド参照をアクセサに置き換える。
- 後方互換のない変更として `CHANGES.md` の `## develop` に `[CHANGE]` を追記する。

## 完了条件

- `PlaybackFrame` のフィールドが private であり、`data()` / `frames()` / `channels()` / `sample_rate()` / `format()` のアクセサ経由でのみ読み取れる。
- 構造体リテラルによる直接構築がコンパイルエラーになる。
- `src/playback_ffi.rs` / `src/playback_wasapi.rs` と `pbt/tests/prop_common.rs` の `PlaybackFrame` のフィールド参照がアクセサに置き換わっている。
- `PlaybackFrame` と `from_s16` / `from_f32` の rustdoc に、フィールドの意味と整合条件が書かれている。
- `CHANGES.md` の `## develop` に `[CHANGE]` が追記されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が、CI の全構成 (PulseAudio / PipeWire / CoreAudio / WASAPI) で通る。
