# audio_pipewire.c pw_init の戻り値未チェックと CHANGES.md の記載不整合を修正する

- Created: 2026-07-30
- Completed: 2026-10-23
- Branch: feature/fix-pipewire-pw-init-check
- Polished: {YYYY-MM-DD}

## 目的

`audio_pipewire.c` の `pw_init` 呼び出し 3 箇所で戻り値がチェックされていない問題を修正し、CHANGES.md の記載とコードの実態を整合させる。

## 現状

CHANGES.md の 2026.2.0 セクションに「[FIX] pw_init の戻り値未チェックを修正する」と記載されているが、コードでは修正が存在しない。

`src/audio_pipewire.c` 内の 3 箇所すべてで `pw_init(NULL, NULL)` の戻り値を無視している:

1. `audio_pipewire_enumerate_devices` 関数内
2. `audio_pipewire_session_create` 関数内
3. `audio_pipewire_playback_session_create` 関数内

`pw_init` は PipeWire ライブラリの初期化を行い、失敗時は負の値を返す。初期化に失敗した状態で PipeWire API を呼び出すと未定義動作やクラッシュの可能性がある。

## 設計方針

`pw_init` の戻り値をチェックし、失敗時はエラーを返す。`pw_init` は複数回呼んでも安全（冪等）であるため、3 箇所すべてでチェックする。

## 完了条件

`pw_init` の戻り値が 3 箇所すべてでチェックされ、失敗時にエラーが返されること。CHANGES.md の記載とコードの実態が整合すること。

## 解決方法

本 issue の記述は誤りであり、修正は行わない。

- `pw_init` の戻り値型は `void` であり、失敗時の戻り値は存在しない
- 0014 で同じ誤認に基づく修正を試み、`void value not ignored as it ought to be` のコンパイルエラーで revert 済み
- CHANGES.md の 2026.2.0 の記載は、revert 後に残った古い記述である
- CHANGES.md はリリース済みの過去の記述のため、本 issue では修正しない
- 0014 の結論（戻り値チェックは不要）を再確認した
