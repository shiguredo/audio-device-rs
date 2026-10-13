# WASAPI キャプチャのタイムスタンプ計算で i64 オーバーフローが発生する

- Created: 2026-10-13
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasapi-timestamp-overflow
- Polished: {YYYY-MM-DD}

## 目的

Windows (WASAPI) の音声キャプチャで `QueryPerformanceCounter` の値をマイクロ秒へ変換する計算が i64 オーバーフローを起こす問題を修正する。debug ビルドではパニックし、release ビルドでは不正なタイムスタンプが生成されるため、実マイクを使うアプリケーションが長時間稼働の Windows 環境で動作しなくなる。

## 現状

`src/capture_wasapi.rs` の `capture_thread_func` 内で、パケットごとのタイムスタンプを以下の式で計算している。

```rust
let timestamp_us = if frequency > 0 {
    (counter * 1_000_000) / frequency
} else {
    0
};
```

`counter` は `QueryPerformanceCounter` が返す i64、`frequency` は `QueryPerformanceFrequency` が返す i64 である。

Windows の QPC カウンターは単調増加で、システム起動からの経過時間に比例する。一般的な `frequency` は 10,000,000 (10 MHz) で、`counter` は起動から約 10.7 日で約 9.22e12 に達する。このとき `counter * 1_000_000` は i64 の最大値 (約 9.223e18) を超える。

- debug ビルド (overflow-checks 有効) では "attempt to multiply with overflow" でパニックする
- release ビルド (overflow-checks 無効) ではラップアラウンドし、負の値を含む不正な `timestamp_us` が生成される

実際に、Windows の実マイクを使うアプリケーションで、QPC カウンターが閾値を超えた状態のときに `capture_thread_func` がパニックすることを確認した。WASAPI の実キャプチャ経路を通らない場合 (疑似デバイス利用時など) は発生しない。

## 設計方針

- 中間計算を i128 で行い、結果を i64 へ戻す。`timestamp_us` はマイクロ秒であり、i64 の範囲 (約 292,000 年) に収まるため i128 からのキャストは安全である
- 変換処理を `src/capture_wasapi.rs` 内の純粋関数へ切り出し、境界値を含む単体テストを追加する。WASAPI 実機を必要とせず、オーバーフロー条件を再現できるようにするため

## 解決方法

`capture_thread_func` の変換式を以下に変更する。

```rust
let timestamp_us = if frequency > 0 {
    ((counter as i128) * 1_000_000 / (frequency as i128)) as i64
} else {
    0
};
```

変換処理を純粋関数 (例: `qpc_counter_to_micros(counter: i64, frequency: i64) -> i64`) として切り出し、`capture_thread_func` からはその関数を呼ぶ。`#[cfg(test)] mod tests` に以下を追加する。

- `counter * 1_000_000` が i64 の最大値を超える値 (例: `i64::MAX / 1_000_000 + 1`) を渡してもパニックせず、期待するマイクロ秒が返ること
- `frequency <= 0` の場合に 0 が返ること
- 通常値 (例: `counter = frequency` で 1 秒) で 1,000,000 が返ること

## 完了条件

- 変換計算が i128 を経由し、`counter` が大きな値でもオーバーフローしないこと
- 境界値 (i64 を超える乗算) の単体テストが追加され、パスすること
- Windows を含む `cargo test --workspace` がパスすること
- `CHANGES.md` の `## develop` に `[FIX]` エントリが追記されていること
