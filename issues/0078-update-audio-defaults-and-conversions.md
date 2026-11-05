# 音声設定の既定値と変換の契約を統一する

- Created: 2026-11-02
- Completed: {YYYY-MM-DD}
- Branch: feature/update-audio-defaults-and-conversions
- Polished: {YYYY-MM-DD}

## 目的

sample_rate / channels の既定値の実装が散在している状態と、フォーマット変換のスケール・endian の契約が不明な状態を整理する。

## 現状

- 「`sample_rate` が 0 以下なら 48000、`channels` が 0 以下ならキャプチャ 1 / 再生 2」という同じ契約が、`audio.h` のコメント、Rust の doc、README、PulseAudio / PipeWire / CoreAudio の各 `*_session_create`、WASAPI の capture / playback 実装の 9 箇所で独立に実装・記述されている。片方だけ変更しても他が追随しない。
- `write_playback_frame_to_buffer` の F32 → S16 は `* 32767.0`、S16 → F32 は `/ 32768.0` でスケールが非対称である。`-1.0` は `-32767` になり、往復で最下位ビットが一致しない。変換式と丸め方法が doc に無い。
- `PlaybackFrame` の `data` は `to_le_bytes` で格納するが、変換関数は native-endian で読む。サポート対象はすべて little-endian だが、データ表現の契約がどこにも書かれておらず、big-endian ターゲットでは無警告でバイト順が壊れる。
- 32 bit ターゲットでは `dst_channels * bytes_per_sample` が wrap してゼロ除算になり得るが、サポート方針が書かれていない。
- C 側の `int frames = (int)(length / frame_size)` は `length` が int の範囲を超えると切り詰める。実際のパケット長では到達しにくいが、境界の契約が無い。

## 設計方針

- 既定値の正本を Rust 側の 1 箇所に置き、C 側へは確定値を渡す。`audio.h` のコメントを「呼び出し側が確定値を渡す」契約へ変更する。
- 変換のスケールを対称 (`* 32768.0` と `clamp(-32768.0, 32767.0)` など) に揃えるか、現行のスケールを doc に明記する。丸め方法も書く。
- `PlaybackFrame` の doc に little-endian であることと対象プラットフォームの前提を書く。`to_ne_bytes` に統一するか、little-endian 前提を明示する。
- 32 bit / big-endian のサポート方針を README か Cargo.toml のコメントに明記する。

## 完了条件

- 既定値の実装が 1 箇所に集約され、C 側は確定値を受け取る。
- 変換のスケールと丸め、endian の契約が doc に書かれている。
- 往復変換のテストが契約に沿った期待値を持つ。
- `cargo test --workspace` が通る。
