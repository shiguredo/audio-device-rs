# 変更履歴

## develop

- [FIX] macOS のデバイス列挙で `malloc` と `CFStringGetCString` の失敗を検証して不正な C 文字列の生成を防止する
  - @voluntas
- [FIX] PipeWire の process callback で `n_datas` と `chunk` の NULL 検証を追加する
  - @voluntas
- [FIX] PipeWire の `pw_properties_new` 失敗時の NULL チェックを追加する
  - @voluntas
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
