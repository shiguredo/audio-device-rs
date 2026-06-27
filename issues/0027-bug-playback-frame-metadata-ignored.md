# PlaybackFrame の frames / channels / sample_rate が無視される

- Priority: Medium
- Created: 2026-06-28
- Completed: {YYYY-MM-DD}
- Model: Kimi K2.7 Code
- Branch: feature/fix-playback-frame-metadata-ignored
- Polished: {YYYY-MM-DD}

## 目的

`PlaybackFrame` の `frames` / `channels` / `sample_rate` が再生実装で無視されている問題を解決し、API 利用者が誤ったデータを渡しても意図しない再生結果にならないようにする。

## 優先度根拠

`PlaybackFrame` は `frames`, `channels`, `sample_rate` を持つ構造体であり、利用者はこれらが尊重されると期待する。現状ではこれらが無視されるため、デバイスと異なるチャンネル数やフレーム数を持つデータを渡すと、無音埋めやコピーサイズがずれ、ノイズや再生速度の異常につながる可能性がある。

## 現状

`src/playback_windows.rs`、`src/playback.rs`、`src/audio_c.m`、`src/audio_pulse.c`、`src/audio_pipewire.c` の再生コールバック処理は、主に `frame.data.len()` とデバイス側の `channels`・`format` を使ってコピー・変換を行う。`PlaybackFrame.frames`、`PlaybackFrame.channels`、`PlaybackFrame.sample_rate` は参照されていない。

例えば `src/playback.rs:164` では `sample_size = channels as usize * bytes_per_sample` としてデバイス側の `channels` を使用しており、`frame.channels` は無視されている。

## 設計方針

以下のいずれかの方針を採用する。

1. `frame.frames` / `frame.channels` / `frame.sample_rate` がデバイス側パラメータと整合していることを検証し、不一致なら無音を書くか 0 フレームを返す
2. `frame` のメタデータを信頼してデバイス側パラメータを上書きしないようにする（ただしデバイス側のフォーマットは変更できないため難しい）
3. `PlaybackFrame` から不要なフィールドを削除し、ドキュメントで「デバイス側フォーマットに合わせた `data` を渡すこと」を明記する

当面は方針 1 が最も安全で後方互換性を損なわない。

## 完了条件

- `PlaybackFrame` のメタデータと実際の `data` 長に矛盾がある場合、無音が再生されるかエラーが返されること
- 正常系（`from_s16` / `from_f32` を正しく使用した場合）の動作が変わらないこと
- 各プラットフォームで一貫した動作をすること

## 解決方法

各バックエンドの再生コールバックで、まず `frame.frames * frame.channels * bytes_per_sample` が `frame.data.len()` と一致するかを確認する。一致しない場合は無音を書き込み 0 フレームを返す。

`sample_rate` はデバイス側で固定されているため無視してよいが、理由をコメントとして残す。
