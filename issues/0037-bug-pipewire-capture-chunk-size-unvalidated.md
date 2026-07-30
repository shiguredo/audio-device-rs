# audio_pipewire.c キャプチャ on_process の chunk->size 境界未検証を修正する

- Created: 2026-07-30
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pipewire-capture-chunk-size
- Polished: {YYYY-MM-DD}

## 目的

`audio_pipewire.c` のキャプチャ側 `on_process` コールバックで `chunk->size` を `maxsize` に対して検証していないため、バッファ外読み取り（OOB read）が発生する可能性を修正する。

## 現状

キャプチャ側 `on_process` では `spa_buf->datas[0].chunk->size` をそのまま読み取りサイズとして使用している。`maxsize` に対する検証がない。

```c
const void* data = spa_buf->datas[0].data;
uint32_t size = spa_buf->datas[0].chunk->size;
// size が maxsize を超えていてもそのまま callback に渡す
```

再生側 `playback_on_process` では `spa_buf->datas[0].maxsize` を使用してバッファサイズを決定しており、`chunk->size` を直接読み取りサイズには使っていない。

PipeWire サーバが `chunk->size > maxsize` を設定した場合、`data` ポインタから `size` バイトの読み取りがバッファ外に及ぶ。

## 設計方針

`chunk->size` が `maxsize` を超える場合に `maxsize` にクランプする。

## 完了条件

`on_process` 内で `chunk->size` が `maxsize` を超えないことが保証されること。

## 解決方法

`src/audio_pipewire.c` の `on_process` 関数内で、`chunk->size` 読み取り後にクランプを追加する:

```c
uint32_t size = spa_buf->datas[0].chunk->size;
if (size > spa_buf->datas[0].maxsize) {
    size = spa_buf->datas[0].maxsize;
}
```
