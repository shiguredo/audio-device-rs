# PulseAudio デバイス列挙でコンテキスト切断時に無限ループするのを防ぐ

Created: 2026-06-07
Model: deepseek-v4-pro
Polished: 2026-06-07

## カテゴリ

fix

## 概要

`audio_pulse.c` の `audio_enumerate_devices()` において、source/sink 列挙の完了待機ループが `enum_ctx.done` カウンタのみで進行するため、PulseAudio サーバが列挙中にクラッシュまたは切断された場合に無限ループに陥る。

## 対象箇所

- `src/audio_pulse.c:217-221` — source 列挙待機ループ
- `src/audio_pulse.c:238-242` — sink 列挙待機ループ
- `src/audio_pulse.c:149-155` — `enumerate_state_callback` (root cause)

## 根拠

最初の接続待機ループ (行 189-201) は `PA_CONTEXT_FAILED` / `PA_CONTEXT_TERMINATED` を検査して脱出しているが、source/sink 列挙待機ループでは列挙完了を `enum_ctx.done` でのみ判定している。`enumerate_state_callback` (行 149-155) は状態変更を `(void)state` で捨てているため、コンテキストが FAILED/TERMINATED になっても検出されず、`done` がインクリメントされない。

## 再現手順

1. PulseAudio サーバを起動する
2. `audio_enumerate_devices()` を呼び出す
3. 列挙中に PulseAudio サーバを `pulseaudio --kill` で停止する
4. 列挙待機ループが永遠に完了しない

## 対応方針

列挙待機ループ内で `pa_context_get_state(context)` をチェックし、`PA_CONTEXT_FAILED` または `PA_CONTEXT_TERMINATED` の場合は脱出する。以下のコードパターンを両方のループに適用する:

```c
while (enum_ctx.done < EXPECTED) {
    pa_context_state_t state = pa_context_get_state(context);
    if (state == PA_CONTEXT_FAILED || state == PA_CONTEXT_TERMINATED) {
        break;
    }
    if (pa_mainloop_iterate(mainloop, 1, &ret) < 0) {
        break;
    }
}
```

## CHANGES.md への追記

`## develop` セクションに以下のエントリを追記する:

```
- [FIX] PulseAudio デバイス列挙でコンテキスト切断時に無限ループする問題を修正する
  - @ユーザー名
```

## テスト戦略

- PulseAudio サーバの停止を伴うテストは CI 環境で再現が困難なため、単体テストは行わない
- コードレビューによるロジックの検証を主とする
- 手動での動作確認: デバイス列挙中に PulseAudio を `pulseaudio --kill` で停止させ、無限ループに陥らないことを確認する
