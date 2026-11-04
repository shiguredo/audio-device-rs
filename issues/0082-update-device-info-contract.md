# デバイス情報の意味と既定値を定義する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-device-info-contract
- Polished: 2026-11-04

## 目的

デバイスの `channels` / `sample_rate` の意味と、取得できなかった場合の既定値をバックエンド間で定義し、利用者が既定値と実値を判別できるようにする。あわせて、デバイス列挙時に未知の device_type や NULL のデバイスポインタをスキップした件数を利用者が取得できるようにする。

`name` / `unique_id` の意味とエラー契約は issue 0072 を正本とし、それらの成功時非空と `Error::NullPointer` 経路のテストは issue 0065 の範囲とする。本 issue では `channels` / `sample_rate` の契約と、列挙時のスキップの扱いに限定する。

## 現状

- デバイスの `channels` / `sample_rate` の意味がバックエンドごとに異なる。CoreAudio は nominal sample rate、WASAPI は mix format、PulseAudio は source / sink の sample spec、PipeWire は node のプロパティを使っている。`src/device.rs` の `AudioDevice::channels` / `sample_rate` の rustdoc は「チャンネル数を取得する」「サンプルレートを取得する」としか書いておらず、値の意味と既定値の扱いが分からない。
- 取得に失敗した場合の既定値も不統一である。CoreAudio は `channels` が 0 以下の場合に 2 ch、`sample_rate` が 0 以下の場合に 48000 Hz を使う。PulseAudio は入力 1 ch・出力 2 ch、サンプルレート 48000 Hz を使う。PipeWire は 1 ch / 48000 Hz を使う。WASAPI は `get_device_format` が失敗した場合に 2 ch / 48000 Hz を使う（issue 0060 の完了後は、形式を取得できないデバイスを列挙から除外する予定である）。`AudioDevice::channels()` / `sample_rate()` は `i32` を返すため、利用者は実際の値と既定値を区別できない。
- CI の `device_list` / `device_info` の `jq` チェックは、`channels` / `sample_rate` については `> 0` だけを見ており、既定値でも通ってしまう。名前・ID の非空や件数の一致も同時に確認しているが、既定値と実値の区別はしていない。
- `FfiDeviceListImpl::enumerate` は、未知の device_type のデバイスと NULL のデバイスポインタを `filter_map` の中で黙って捨てる。`AudioDeviceList` はスキップ件数を公開しないため、利用者は列挙結果が生のデバイス件数より少ない理由を知れない。要求された device_type によるフィルタは利用者が指定した意図的な除外であり、スキップとは区別する。
- `AudioDevice::name()` / `unique_id()` の `Err(Error::NullPointer(...))` 経路は、`src/device_ffi.rs` の `FfiDeviceImpl::name` / `unique_id` にのみ存在する。実機の C バックエンドは非 NULL の名前と ID を返すため、実機テストではこの経路に到達できない。この経路のテストは issue 0065 の範囲である。

## 関連 issue

- `issues/0060-bug-wasapi-device-and-session-correctness.md`: WASAPI のデバイス列挙で既定値を捏造せず、形式を取得できないデバイスをスキップする。0082 の WASAPI の既定値フラグは 0060 の完了後の挙動に合わせる。
- `issues/0065-test-add-coverage-gaps.md`: `AudioDevice::name()` / `unique_id()` の成功時非空と `Err(Error::NullPointer(...))` 経路のテスト。
- `issues/0072-doc-public-api-contracts.md`: `name` / `unique_id` の rustdoc 契約と `# Errors`。0082 は `channels` / `sample_rate` の rustdoc に限定する。
- `issues/0078-update-audio-defaults-and-conversions.md`: セッション設定の `sample_rate` / `channels` の既定値。デバイス列挙の既定値は 0082 の範囲である。
- `issues/0079-refactor-deduplicate-platform-code.md`: `examples/device_list.rs` と `examples/device_info.rs` の JSON 生成の共通化。0082 で追加するフィールドは、0079 の共通化後も同じ契約にする。
- `issues/0080-remove-dead-code-and-unused-configs.md`: C のデバイスアクセサの NULL 分岐を削除する。0082 の `FfiDeviceImpl::name` / `unique_id` の `Error::NullPointer` 経路の扱いに影響する。
- `issues/0081-test-remove-device-ffi-test-stubs.md`: 未知の device_type と NULL デバイスポインタの純粋関数テスト。0082 のスキップ件数の扱いに合わせる。

## 設計方針

- `src/device.rs` の `AudioDevice::channels` / `sample_rate` の rustdoc に、値の意味を書く。`channels` / `sample_rate` はバックエンドが報告するデバイスの代表値であり、バックエンドが実際の値を取得できなかった場合はバックエンド既定値を使う。既定値の数値はバックエンドごとに異なるため、数値の統一は必須としない。
- `AudioDevice::channels_is_default() -> bool` / `sample_rate_is_default() -> bool` を追加する。`true` は、対応する値がバックエンド既定値であり、実際のデバイス値ではないことを表す。`false` は、バックエンドから取得した実値を表す。C バックエンドのデバイス構造体に既定値を使ったかどうかのフラグを追加し、`DeviceOps` と FFI 関数で返す。WASAPI は `WasapiDeviceImpl` に同じフラグを追加する。issue 0060 の完了後は、WASAPI は形式を取得できないデバイスを列挙しないため、フラグは常に `false` になる。0060 の完了前は、`get_device_format` が失敗した場合に `true` を返す。
- `AudioDeviceList::skipped_count() -> Option<usize>` を追加する。FFI バックエンドの `AudioDeviceList` では、`FfiDeviceListImpl::enumerate` が未知の device_type または NULL のデバイスポインタを検出してスキップした件数を `Some(count)` で返す。WASAPI では `None` を返す。要求された device_type によるフィルタはスキップに含めない。スキップした理由は rustdoc に書く。列挙全体はエラーにせず、有効なデバイスだけを返す。
- `FfiDeviceListImpl::enumerate` の `filter_map` を、スキップ件数を数えられるループに変更する。NULL デバイスポインタと未知の device_type を検出した場合は、そのデバイスをスキップして件数を増やす。フィルタ不一致の場合は件数を増やさない。`FfiDeviceListImpl` はスキップ件数を保持し、`AudioDeviceList::from_ffi` で `AudioDeviceList` に引き継ぐ。
- `name` / `unique_id` の rustdoc とエラー契約は issue 0072 を正本とし、本 issue では変更しない。成功時非空と `Error::NullPointer` 経路のテストは issue 0065 の範囲とし、本 issue では重複して実装しない。
- `examples/device_list.rs` / `examples/device_info.rs` の JSON に `channels_is_default` / `sample_rate_is_default` を追加する。`device_list` のトップレベルと `device_info` の `statistics` に `skipped_count` を追加する。`skipped_count` は `null` または 0 以上の整数とする。CI の `.github/workflows/ci.yml` の `jq` チェックは、`channels_is_default` / `sample_rate_is_default` が boolean であること、`channels` / `sample_rate` が正であること、`skipped_count` が `null` または 0 以上の整数であることを検証する。既定値の数値はバックエンドごとに異なるため、CI では値そのものを固定しない。
- `CHANGES.md` の `## develop` に `[ADD]` エントリを追加する。`AudioDevice::channels_is_default()` / `sample_rate_is_default()` と `AudioDeviceList::skipped_count()` の追加を書く。ドキュメントだけの変更は `CHANGES.md` に含めない。
- `FfiDeviceListImpl::enumerate` のスキップ件数の集計は、NULL、未知の device_type、フィルタを入力に取る純粋関数に切り出してテストする。issue 0081 が `select_device_type` / `non_null_device` を追加する場合は、0081 の純粋関数テストの枠組みを使う。0082 が先に実装される場合は、0082 で同等のテストを追加する。

## 完了条件

- `AudioDevice::channels` / `sample_rate` の意味と既定値が rustdoc に書かれている。
- `AudioDevice::channels_is_default()` / `sample_rate_is_default()` が追加され、その契約が rustdoc に書かれ、利用者が既定値と実値を判別できる。
- `AudioDeviceList::skipped_count()` が追加され、FFI バックエンドで未知の device_type / NULL ポインタのためにスキップした件数を取得できる。フィルタによる除外は件数に含まれないことと、WASAPI では `None` を返すことが rustdoc に書かれている。
- 未知の device_type / NULL ポインタの扱いとスキップ件数の集計が rustdoc に書かれ、0081 の純粋関数テストまたは 0082 のテストで固定されている。
- `device_list` / `device_info` の JSON に新しいフィールドが追加され、CI の `jq` チェックが更新されている。
- `CHANGES.md` の `## develop` に `[ADD]` エントリが追加されている。
- `cargo fmt --all -- --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace` が通る。CI の全構成（CoreAudio / PulseAudio / PipeWire / WASAPI）で確認する。
