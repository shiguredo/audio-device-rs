# PulseAudio デバイス列挙でコンテキスト切断時に無限ループするのを防ぐ

Created: 2026-06-07
Model: deepseek-v4-pro

## カテゴリ

bug

## 概要

`audio_pulse.c` の `audio_enumerate_devices()` において、source/sink 列挙の完了待機ループが `enum_ctx.done` カウンタのみで進行するため、PulseAudio サーバが列挙中にクラッシュまたは切断された場合に無限ループに陥る。

## 対象箇所

- `src/audio_pulse.c:217-221` — source 列挙待機ループ
- `src/audio_pulse.c:238-242` — sink 列挙待機ループ

```c
while (enum_ctx.done < 1) {
    if (pa_mainloop_iterate(mainloop, 1, &ret) < 0) {
        break;
    }
}
```

## 根拠

最初の接続待機ループ (`L189-201`) は `PA_CONTEXT_FAILED` / `PA_CONTEXT_TERMINATED` を検査して脱出しているが、source/sink 列挙待機ループでは列挙完了を `enum_ctx.done` でのみ判定している。`enumerate_state_callback` (`L149-155`) は状態変更を `(void)state` で捨てているため、コンテキストが FAILED/TERMINATED になっても検出されず、`done` がインクリメントされない。

## 再現条件

- デバイス列挙中に PulseAudio サーバが停止またはクラッシュした場合

## 対応方針

列挙待機ループ内で `pa_context_get_state(context)` をチェックし、`PA_CONTEXT_FAILED` または `PA_CONTEXT_TERMINATED` の場合は脱出する。または `enumerate_state_callback` を適切に実装して状態変更を検出する。
