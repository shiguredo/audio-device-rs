# デバイス情報の意味と既定値を定義する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-device-info-contract
- Polished: {YYYY-MM-DD}

## 目的

デバイスの `channels` / `sample_rate` / `name` / `unique_id` の意味と、取得できなかった場合の扱いをバックエンド間で定義する。

## 現状

- デバイスの `channels` / `sample_rate` の意味がバックエンドごとに異なる。CoreAudio は nominal sample rate、WASAPI は mix format、PulseAudio は source / sink の sample spec、PipeWire は node のプロパティを使っている。doc は「取得する」としか書いていない。
- 取得に失敗した場合の既定値も不統一である。CoreAudio は 1 ch でも 2 ch に、PulseAudio は入力 1 ch・出力 2 ch に、PipeWire は 1 ch に丸め、サンプルレートは 48000 に丸める。`AudioDevice::channels()` / `sample_rate()` の利用者は、実際の値と既定値を区別できない。
- CI の `device_list` / `device_info` の `jq` チェックは `channels > 0` と `sample_rate > 0` しか見ないため、既定値でも通ってしまう。
- `FfiDeviceListImpl::enumerate` は未知の device_type や NULL のデバイスポインタを黙って捨てるため、列挙結果が件数より少なくなる理由を利用者が知れない。
- `AudioDevice::name()` / `unique_id()` の `Err(Error::NullPointer(...))` 経路がテストで検証されていない。

## 設計方針

- `channels` / `sample_rate` が「デバイスの代表値」であり、取得できない場合はバックエンド既定値であることを doc に書く。または `Option<i32>` にして不明を表現する。
- バックエンド間で既定値を統一するか、既定値を使ったことを利用者が判別できる API を検討する。
- 未知の device_type や NULL デバイスポインタは、黙って捨てるのではなくエラーまたは記録として扱う。
- `name()` / `unique_id()` の成功時非空とエラー経路を実機テストで検証する。
- CI の `jq` チェックに既定値と実値の区別を追加するか、既定値に依存しない検証にする。

## 完了条件

- デバイス情報の意味と既定値が doc に書かれている。
- 既定値と実値を利用者が区別できる。
- 未知の device_type / NULL ポインタの扱いが定義され、テストで固定されている。
- `cargo test --workspace` が通る。
