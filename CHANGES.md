# 変更履歴

## develop

- [FIX] Windows 再生経路のフォーマット変換で `Vec<u8>` を `&[f32]` / `&[i16]` に再解釈する際のアライメント違反による UB を修正する
  - @voluntas
- [FIX] `SendPtr<T>` の blanket `Send` impl を使用する具体型のみに制限する
  - @voluntas
- [FIX] Windows のキャプチャ・再生コールバックで panic を `catch_unwind` で捕捉する
  - @voluntas
