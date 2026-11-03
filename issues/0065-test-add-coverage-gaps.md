# テストカバレッジの不足を解消する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/add-test-coverage-gaps
- Polished: 2026-11-04

## 目的

公開 API と FFI 境界の未検証箇所を洗い出し、リグレッションを検出できるテストを追加する。

## 現状

- `src/capture_ffi.rs` の `frame_callback` には単体テストが 1 件も無い。同じ境界の `playback_callback` には NULL・境界値・変換・無音のテストが 14 件あり、非対称である。`frame_callback` の NULL 引数、`frames <= 0`、`channels <= 0`、未知フォーマット、サイズ計算のオーバーフロー分岐が未検証。サイズ計算のオーバーフロー分岐は、64 bit 環境では `frames` と `channels` が i32 の正の値である限り `usize` の範囲を超えないため到達しない。32 bit 環境でのみ到達する。
- `tests/session_lifecycle.rs` の `wait_for_process` は `recv_timeout` のタイムアウトを無視して続行するため、`start()` が `Ok` を返してもコールバックが一度も来ない実装を検出できない。10 サイクルの再開始テストも最終的な `assert_no_process_after_stop` も、コールバックが 0 回なら自明に通る。macOS のキャプチャはマイクへのアクセス許可が無いため `skip_capture()` でスキップされる。
- `tests/session_lifecycle.rs` の `input_device_count` / `output_device_count` と `tests/test_device_timing.rs` の列挙ヘルパーは件数または空判定しか見ておらず、列挙したデバイスの `device_type()` が期待どおりかを検証していない。WASAPI のフィルタ不具合のような問題を検出できない。CoreAudio は同じ `unique_id` を持つ入力と出力の 2 エントリを返すため、重複の判定は `unique_id` 単独ではなく `(unique_id, device_type)` の組で行う必要がある。
- `sample_rate: 0` / `channels: 0` の既定値クランプを確認するテストが無い。既定値の実装がバックエンドごとに存在するため回帰しやすい。既定値は 48000 Hz、キャプチャ 1 ch、再生 2 ch である。
- `AudioDevice::name()` / `unique_id()` が `Ok` かつ非空であること、`Err(Error::NullPointer(...))` 経路が Rust の実機テストで検証されていない。`Err(Error::NullPointer(...))` は FFI バックエンドの `device_ffi.rs` にのみ存在し、実機で C 側が NULL を返さない限り到達しない。CI の device-test は examples の JSON で非空を確認している。
- README のコード例はコンパイル検証の対象になっていない。examples は `cargo test --workspace` でコンパイルされ、CI の device-test で `device_list` / `device_info` が実行される。

## 設計方針

- `capture_ffi.rs` に `playback_ffi.rs` と同種のテストを追加する。NULL 引数、`frames <= 0`、`channels <= 0`、未知フォーマット、正常系のデータ長・フィールドを検証する。`frame_callback` は `sample_rate` を検証しないため、`sample_rate <= 0` の拒否テストは追加しない。サイズ計算のオーバーフロー分岐は 64 bit では到達しないため、32 bit 限定のテストにするか、`checked_mul` の結果を直接検証する。
- `session_lifecycle` で、実行されるキャプチャ・再生の各テストについて少なくとも 1 サイクルはコールバック到着を必須にし、デバイス 0 件のスキップとコールバック不来を区別する。macOS のキャプチャは `skip_capture()` でスキップされるため、再生側でコールバック到着を必須にする。
- 列挙結果の `device_type()` 全件一致と、全デバイス列挙との和集合を検証する。入力列挙が入力のみ、出力列挙が出力のみを返し、全列挙が両者の和集合と一致することを確認する。重複は `(unique_id, device_type)` の組で判定し、CoreAudio で同じ `unique_id` を持つ入力と出力の 2 エントリが返ることを許容する。0064 で `tests/test_device_timing.rs` は `tests/device_timing.rs` にリネームされ、列挙ヘルパーは bool を返す形に変更されるため、列挙検証はテスト内で `AudioDeviceList` を直接列挙して行う。
- 既定値クランプの実機テストを追加する。`AudioCapture::new` / `AudioPlayback::new` の直後に `sample_rate()` / `channels()` を検証し、キャプチャは 48000 Hz / 1 ch、再生は 48000 Hz / 2 ch になることを確認する。start は不要である。デバイスが無い場合はスキップし、列挙エラーと `new` のエラーは失敗させる。
- デバイス名・ID の `Ok` かつ非空を実機テストで検証する。`Err(Error::NullPointer(...))` 経路は `device_ffi.rs` の `#[cfg(test)]` でテスト用の `DeviceOps` を使った単体テストとして検証する。
- README の代表サンプルを doctest にするか examples から生成し、コンパイル検証の対象にする。doctest にする場合は、`?` をトップレベルで使っているため `fn main() -> Result<...>` で包み、`enable_default_*` が無い feature 構成でもコンパイルできるようにする。

## 完了条件

- `frame_callback` の NULL 引数、`frames <= 0`、`channels <= 0`、未知フォーマット、正常系のデータ長・フィールドにテストがある。サイズ計算のオーバーフロー分岐は 32 bit で到達することを確認するか、`checked_mul` の結果を直接検証している。
- コールバックが来ない実装が `session_lifecycle` で失敗する。デバイス 0 件のスキップとコールバック不来が区別されている。
- 入力列挙が入力のみ、出力列挙が出力のみを返し、全列挙が両者の和集合と一致することが検証されている。重複は `(unique_id, device_type)` の組で判定されている。
- `sample_rate: 0` / `channels: 0` の既定値が、キャプチャ 48000 Hz / 1 ch、再生 48000 Hz / 2 ch になることが検証されている。
- `AudioDevice::name()` / `unique_id()` が `Ok` かつ非空であることが実機テストで検証されている。
- `Err(Error::NullPointer(...))` 経路が `device_ffi.rs` の単体テストで検証されている。
- README の代表サンプルがコンパイル検証の対象になっている。
- 追加したテストがローカルと CI で通る。
