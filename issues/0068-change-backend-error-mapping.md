# バックエンドのエラーコードを公開エラーへ写像する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/change-backend-error-mapping
- Polished: {YYYY-MM-DD}

## 目的

C バックエンドが返すエラーコードの情報を公開 API から観測できるようにし、原因切り分けを可能にする。

## 現状

- `FfiCaptureImpl::start` / `FfiPlaybackImpl::start` は戻り値の負値をすべて `Error::SessionStartFailed` にしている。`audio_pipewire.h` の `AudioPipeWireResult` は 12 個の負値コードを定義し「負の値は AudioPipeWireResult に対応する」と契約しているが、Rust からは区別できない。
- `FfiDeviceListImpl::enumerate` は C 側の負の戻り値をすべて `Error::DeviceAccessDenied` にしている。PulseAudio のサーバ未起動 (-3) やメモリ確保失敗 (-2)、PipeWire のタイムアウト (-9) が同じエラーになる。
- `Error::DeviceAccessDenied` の Display は "audio access denied" であり、原因と一致しないケースがある。
- `Error::ComInitFailed` の Display は "thread has incompatible apartment model" に固定されているが、`CO_E_NOTINITIALIZED` にも使われる。

## 設計方針

- `Error` にバックエンド利用不可・タイムアウトなどを表す variant を追加し、C の戻り値から変換する。公開 enum の variant 追加は後方互換のない変更として `CHANGES.md` に `[CHANGE]` で記載する。
- C の戻り値の意味をバックエンドごとに定義し、`CaptureOps` / `PlaybackOps` / `DeviceOps` の戻り値から変換する。
- 詳細コードを保持しない場合は、C ヘッダの「負の値の意味はバックエンド固有」という記述と Rust 側の一律変換を整合させる。
- `ComInitFailed` の Display を HRESULT に依存しない表現に直す。

## 完了条件

- サーバ未起動、タイムアウト、権限不足、リソース不足が区別できる。
- C ヘッダのエラーコード定義と Rust の変換が一致している。
- `CHANGES.md` に破壊的変更が記載されている。
- `cargo test --workspace` が通る。
