# 変更履歴

## develop

- [FIX] FFI コールバックの `channels` 未検証とバッファサイズ計算の符号付きオーバーフローを修正する
  - @voluntas
- [FIX] `pbt` クレートの default-features が pipewire ビルドで feature 競合を起こす問題を修正する
  - @voluntas
- [FIX] Windows 再生経路のフォーマット変換で `Vec<u8>` を `&[f32]` / `&[i16]` に再解釈する際のアライメント違反による UB を修正する
  - @voluntas
- [FIX] `SendPtr<T>` の blanket `Send` impl を使用する具体型のみに制限する
  - @voluntas
- [FIX] Windows のキャプチャ・再生コールバックで panic を `catch_unwind` で捕捉する
  - @voluntas
