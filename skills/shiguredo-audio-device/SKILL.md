---
name: shiguredo-audio-device
description: 時雨堂のクロスプラットフォームオーディオデバイスライブラリ shiguredo_audio_device の機能・API リファレンス。macOS (CoreAudio) / Linux (PulseAudio / PipeWire) / Windows (WASAPI) でのデバイス列挙、マイクキャプチャ、スピーカー再生、バックエンド選択 feature、initialize_thread とスレッド契約、セッションガードによる開始・停止、コールバックの境界と実行スレッド、タイムスタンプ、S16 / F32 変換、バックエンドごとの挙動差に関する質問時に使用。
---

# shiguredo_audio_device

macOS / Linux / Windows で共通の API を提供するオーディオデバイスライブラリ。
デバイス列挙、音声キャプチャ (マイク入力)、音声再生 (スピーカー出力) を提供する。

## 特徴

- **共通 API**: macOS (CoreAudio / AudioToolbox)、Linux (PulseAudio / PipeWire)、Windows (WASAPI) を同じ型で扱う
- **バックエンドの共存**: Linux では PulseAudio と PipeWire を同時に有効化し、実行時に明示 API で選べる
- **ランタイム依存クレートなし**: macOS / Linux はシステムライブラリのみ、Windows は Microsoft 公式の `windows` クレートのみに依存する
- **コールバック型 API**: キャプチャはフレームごと、再生はデータ要求ごとにバックエンドが管理するスレッド上でコールバックを直列に呼ぶ
- **セッションガード**: `start()` が返すガードが `AudioCapture` / `AudioPlayback` を借用するため、実行中の破棄・二重 `start`・コールバック内からの停止はコンパイルエラーになる
- **フォーマット変換**: 再生コールバックは S16 / F32 のどちらで返してもよく、バックエンドが要求する形式へ変換して書き込む

## バージョン情報

- crate 名: `shiguredo_audio_device`
- バージョン: 2026.4.0
- Rust Edition: 2024
- 最小 Rust バージョン: 1.95
- ライセンス: Apache-2.0
- リポジトリ: <https://github.com/shiguredo/audio-device-rs>

```toml
[dependencies]
shiguredo_audio_device = "2026.4"
```

## 対応バックエンドと feature

| OS | バックエンド | feature | デフォルト指定 feature | 実装 |
|----|-------------|---------|----------------------|------|
| macOS | CoreAudio | `coreaudio` | `default-coreaudio` | AudioQueue (Objective-C を `cc` でビルド) |
| Linux | PulseAudio | `pulse` | `default-pulse` | `pa_threaded_mainloop` (C を `cc` でビルド) |
| Linux | PipeWire | `pipewire` | `default-pipewire` | `pw_thread_loop` (C を `cc` でビルド) |
| Windows | WASAPI | `wasapi` | `default-wasapi` | `windows` クレート (共有モード、イベント駆動) |

- デフォルト feature は `default-coreaudio` / `default-pulse` / `default-wasapi` で、`default-*` はそれぞれ対応するバックエンド feature を有効にする
- ビルドスクリプトはビルド対象 OS の feature だけを見る。Linux のビルドでは `default-coreaudio` / `default-wasapi` は無視されるため、3 OS 分の `default-*` を同時に指定してよい
- 1 つの OS で `default-*` を 2 つ以上有効にすると、ビルドスクリプトが `Multiple default backends selected. Enable exactly one default-* feature.` で panic する
- macOS / Linux / Windows 以外をターゲットにすると、ビルドスクリプトが `Unsupported target OS` で panic する
- `default-*` が有効なときだけ、デフォルトバックエンドを使う `AudioDeviceList::enumerate*()` / `AudioCapture::new()` / `AudioPlayback::new()` が定義される
- バックエンド feature (`pulse` 等) が有効なときだけ、明示 API (`enumerate_pulse()` / `AudioCapture::new_pulse()` 等) が定義される
- バックエンドが 1 つも有効でない場合、`AudioDevice` / `AudioDeviceList` / `AudioCapture` / `AudioPlayback` とそのセッション型は定義されない
- 共通型 (`AudioFrame`、`PlaybackFrame`、`AudioCaptureConfig` 等)、`Error`、`initialize_thread()` はバックエンドの有無にかかわらず定義される

```toml
[dependencies]
# Linux のデフォルトを PipeWire にし、macOS / Windows は CoreAudio / WASAPI のままにする
shiguredo_audio_device = { version = "2026.4", default-features = false, features = ["default-coreaudio", "default-pipewire", "default-wasapi"] }
```

```toml
[dependencies]
# Linux で PulseAudio と PipeWire を共存させ、デフォルトは PulseAudio のままにする
shiguredo_audio_device = { version = "2026.4", features = ["pipewire"] }
```

## ビルド要件

- 共通: Rust 1.95 以降
- macOS / Linux: `bindgen` を使うため libclang が必要
- macOS: Objective-C のコンパイラ (Xcode / Command Line Tools)。`AudioToolbox` / `CoreAudio` / `Foundation` フレームワークをリンクする
- Linux (PulseAudio): `libpulse-dev` (`pkg-config` で `libpulse` を探す)
  - Ubuntu 22.04 以降のように PipeWire がオーディオサーバーの環境では、実行時に `pipewire-pulse` が動作している必要がある
- Linux (PipeWire): `libpipewire-0.3-dev` (`pkg-config` で `libpipewire-0.3` を探す)、実行時に `pipewire-alsa` と PipeWire デーモン
- Windows: 追加の依存パッケージなし (C コンパイルも `bindgen` もしない)

Linux で USB オーディオデバイスが見えない場合の手順は、リポジトリの Linux 向けドキュメント (USB オーディオデバイスを認識させる手順) にまとまっている。

## アーキテクチャ概要

- 公開型は enum の newtype で、内部でバックエンドごとの実装に振り分ける。macOS / Linux は C / Objective-C のセッション API を FFI で呼び、Windows は Rust から WASAPI を直接呼ぶ
- キャプチャ・再生とも「`new` で構築 (停止状態) → `start` でセッションガードを得る → ガードの `stop` または drop で停止」の流れで使う。停止後は同じオブジェクトで再度 `start` できる
- バックエンドがリソースを確保する時点が異なるため、エラーが返るタイミングも異なる

| バックエンド | `new` で行うこと | `start` で行うこと |
|-------------|------------------|-------------------|
| CoreAudio | AudioQueue の作成、デバイス UID の解決とデバイス設定、10 ms × 3 のバッファ確保 | バッファの投入と `AudioQueueStart` |
| PulseAudio | 設定値とデバイス ID の保存のみ (サーバに接続しない) | mainloop / context / stream を作成して接続し、stream が READY になるまで待つ |
| PipeWire | thread loop と context の作成 (サーバに接続しない) | core に接続して stream を作成し、STREAMING または PAUSED になるまで待つ (約 10 秒でタイムアウト) |
| WASAPI | デバイスの取得、`IAudioClient::Initialize`、イベントハンドルの設定 | `IAudioClient::Start` とワーカースレッドの起動 |

## コア API

### スレッド初期化

| 型 / 関数 | 説明 |
|-----------|------|
| `initialize_thread() -> Result<ThreadGuard>` | 現在のスレッドをライブラリを利用できる状態に初期化する。Windows では COM を MTA で初期化し、macOS / Linux では何もしない |
| `ThreadGuard` | 初期化状態を保持するガード (`!Send` / `!Sync`)。スレッドの終了まで保持する。Windows では drop で `CoUninitialize` を呼ぶ |

- ライブラリを利用するスレッドごとに、そのスレッドの先頭で呼ぶこと。macOS / Linux では何もしないが、同じコードを Windows でも動かすために常に呼ぶ
- Windows で呼び出し元スレッドが STA で初期化済みの場合は `Error::ComInitFailed` を返す。GUI フレームワークのスレッドなど STA のスレッドからは使えないため、別スレッドで利用する
- Windows で初期化していないスレッドからデバイス列挙や `AudioCapture::new` などを呼ぶと `Error::ComInitFailed` になる
- 同じスレッドで複数回呼んでもよい。返されたガードはそれぞれ drop する
- キャプチャ・再生のコールバックを呼ぶバックエンドのスレッドは、ライブラリ側で初期化する

### デバイス列挙

| 型 | 説明 | 主要メソッド |
|----|------|-------------|
| `AudioDeviceList` | デバイス列挙の結果 (`Send + Sync`) | 列挙関数 (下記)、`as_slice() -> &[AudioDevice]`、`len()`、`is_empty()` |
| `AudioDevice` | 1 つのデバイス (`Send + Sync`、`Clone` なし) | `name() -> Result<String>`、`unique_id() -> Result<String>`、`channels() -> i32`、`sample_rate() -> i32`、`device_type()` |
| `AudioDeviceType` | デバイスの種類 (`Debug + Clone + Copy + PartialEq + Eq`) | `Input` (マイク)、`Output` (スピーカー) |

列挙関数はすべて `fn() -> Result<AudioDeviceList>` で、全デバイス / 入力のみ / 出力のみの 3 種類がある。

| 対象バックエンド | 全デバイス | 入力のみ | 出力のみ |
|-----------------|-----------|---------|---------|
| デフォルト (`default-*`) | `enumerate()` | `enumerate_input()` | `enumerate_output()` |
| CoreAudio (`coreaudio`) | `enumerate_coreaudio()` | `enumerate_input_coreaudio()` | `enumerate_output_coreaudio()` |
| PulseAudio (`pulse`) | `enumerate_pulse()` | `enumerate_input_pulse()` | `enumerate_output_pulse()` |
| PipeWire (`pipewire`) | `enumerate_pipewire()` | `enumerate_input_pipewire()` | `enumerate_output_pipewire()` |
| WASAPI (`wasapi`) | `enumerate_wasapi()` | `enumerate_input_wasapi()` | `enumerate_output_wasapi()` |

- `&AudioDeviceList` は `IntoIterator` を実装しており、`for device in &list` で反復できる。`AudioDevice` はリストからの参照としてだけ得られ、リストより長く生存できない
- デバイスが 0 件のときは空のリストを返す (エラーにはならない)
- 列挙に失敗した場合 (PulseAudio / PipeWire のサーバ未起動を含む) は `Error::DeviceAccessDenied` を返す
- `unique_id()` の値を `AudioCaptureConfig::device_id` / `AudioPlaybackConfig::device_id` にそのまま渡すと、そのデバイスを指定できる
- `channels()` / `sample_rate()` は列挙時点のデバイスの値で、キャプチャ・再生の形式には使われない。形式は設定値で決まる
- `name()` / `unique_id()` は macOS / Linux で C 側が NULL を返した場合に `Error::NullPointer` を返す。Windows では失敗しない
- デフォルトデバイスがどれかを問い合わせる API はない。デフォルトデバイスを使うときは `device_id: None` を指定する

### キャプチャ

| 型 | 説明 | 主要メソッド / フィールド |
|----|------|---------------------------|
| `AudioCaptureConfig` | キャプチャ設定 (`Default` のみ実装。`Debug` / `Clone` なし) | `device_id: Option<String>` (`None` はデフォルトデバイス)、`sample_rate: i32` (0 以下は 48000)、`channels: i32` (0 以下は 1) |
| `AudioCapture` | キャプチャ本体 (`Send`、`!Sync`) | `new(config, callback)`、`new_<バックエンド>(config, callback)`、`start(&mut self)`、`config()`、`sample_rate()`、`channels()` |
| `AudioCaptureSession<'a>` | 実行中のセッション (`#[must_use]`、`Send`、`!Sync`) | `stop(self)`、`config()`、`sample_rate()`、`channels()` |
| `AudioFrame<'a>` | コールバックに渡されるフレーム (derive なし) | フィールド (下記)、`to_owned() -> AudioFrameOwned`、`as_s16() -> Option<&[i16]>`、`as_f32() -> Option<&[f32]>` |
| `AudioFrameOwned` | `AudioFrame` の所有版 (`Debug + Clone + PartialEq + Eq`) | `AudioFrame` と同じフィールド (`data` は `Vec<u8>`)、`as_frame() -> AudioFrame<'_>`、`as_s16()`、`as_f32()` |

`AudioFrame` のフィールドはすべて公開されている。

| フィールド | 型 | 意味 |
|-----------|----|------|
| `data` | `&'a [u8]` | インターリーブされた PCM データ |
| `frames` | `i32` | サンプルフレーム数 |
| `channels` | `i32` | チャンネル数 |
| `sample_rate` | `i32` | サンプルレート |
| `format` | `AudioFormat` | サンプル形式 |
| `timestamp_us` | `i64` | パケット先頭フレームの時刻 (マイクロ秒)。詳細は「タイムスタンプ」を参照 |

- `AudioCapture::new(config, callback) -> Result<AudioCapture>` はデフォルトバックエンドを使う。明示 API は `new_coreaudio` / `new_pulse` / `new_pipewire` / `new_wasapi` で、引数と戻り値は同じ
- `start(&mut self) -> Result<AudioCaptureSession<'_>>` で開始する
- `callback` には `FnMut(AudioFrame<'_>) + Send + 'static` を要求する
- `config()` は構築時に渡した設定をそのまま返す。0 以下の値を既定値に置き換えた実際の値は `sample_rate()` / `channels()` で得る
- `as_s16()` / `as_f32()` は、`format` が一致しない、`frames` / `channels` が 0 以下、`data` が `frames * channels` サンプル分に満たない、ポインタのアライメントが合わない、のいずれかで `None` を返す
- `AudioFrame::data` はコールバックの実行中だけ有効。コールバックの外で使う場合は `to_owned()` で `AudioFrameOwned` にする

### 再生

| 型 | 説明 | 主要メソッド / フィールド |
|----|------|---------------------------|
| `AudioPlaybackConfig` | 再生設定 (`Default` のみ実装。`Debug` / `Clone` なし) | `device_id: Option<String>` (`None` はデフォルトデバイス)、`sample_rate: i32` (0 以下は 48000)、`channels: i32` (0 以下は 2) |
| `AudioPlayback` | 再生本体 (`Send`、`!Sync`) | `new(config, callback)`、`new_<バックエンド>(config, callback)`、`start(&mut self)`、`config()`、`sample_rate()`、`channels()` |
| `AudioPlaybackSession<'a>` | 実行中のセッション (`#[must_use]`、`Send`、`!Sync`) | `stop(self)`、`config()`、`sample_rate()`、`channels()` |
| `PlaybackFrame` | 再生コールバックが返すフレーム (`Debug + Clone`) | `data: Vec<u8>`、`frames: i32`、`channels: i32`、`sample_rate: i32`、`format: AudioFormat`、`from_s16()`、`from_f32()` |

- `AudioPlayback::new(config, callback) -> Result<AudioPlayback>` はデフォルトバックエンドを使う。明示 API は `new_coreaudio` / `new_pulse` / `new_pipewire` / `new_wasapi` で、引数と戻り値は同じ
- `start(&mut self) -> Result<AudioPlaybackSession<'_>>` で開始する
- `callback` には `FnMut(i32, i32, i32) -> Option<PlaybackFrame> + Send + 'static` を要求する。引数は `(frames, channels, sample_rate)` で、要求フレーム数・チャンネル数・サンプルレートを表す
- `None` を返すとそのバッファは無音になる
- `PlaybackFrame::from_s16(data: &[i16], channels: i32, sample_rate: i32) -> Result<PlaybackFrame>` / `from_f32(data: &[f32], ...)` は、インターリーブされたサンプル列をリトルエンディアンのバイト列に変換して保持する
  - `channels` が 0 以下なら `Error::InvalidChannels`
  - 要素数が `i32` に収まらなければ `Error::DataTooLarge`
  - 要素数が `channels` の倍数でなければ `Error::InvalidDataLength`

### 共通型

| 型 | 説明 |
|----|------|
| `AudioFormat` | `S16` (符号付き 16 bit 整数)、`F32` (32 bit 浮動小数点数)。`Debug + Clone + Copy + PartialEq + Eq` |
| `Result<T>` | `std::result::Result<T, Error>` のエイリアス |

### エラー型

`Error` は `Debug + Clone + Copy` を derive し、`Display` と `std::error::Error` を実装する。`PartialEq` は実装しないため、判定には `matches!` を使う。`From<std::num::TryFromIntError>` を実装する。

| variant | 発生する場面 |
|---------|-------------|
| `DeviceNotFound` | WASAPI で `device_id` のデバイスが見つからない、またはデフォルトデバイスが無い (`new` 時) |
| `DeviceAccessDenied` | デバイス列挙の失敗 (全バックエンド。サーバ未起動を含む)、WASAPI の `IMMDevice::Activate` の失敗 |
| `SessionCreateFailed` | `new` の失敗。CoreAudio の AudioQueue 作成失敗や `device_id` の UID 未発見、WASAPI の `Initialize` 失敗 (要求形式へ変換できない場合を含む)、PulseAudio / PipeWire のリソース確保失敗 |
| `SessionStartFailed` | `start` の失敗。PulseAudio のサーバ接続失敗や存在しないデバイス名、PipeWire の接続失敗・タイムアウト、CoreAudio の `AudioQueueStart` 失敗、WASAPI の `Start` 失敗やスレッド生成失敗 |
| `ComInitFailed` | Windows で `initialize_thread()` が COM を MTA で初期化できない、または COM を初期化していないスレッドから呼んだ |
| `NullPointer(&'static str)` | macOS / Linux で C 側がデバイス名やデバイス ID に NULL を返した |
| `InvalidChannels` | `PlaybackFrame::from_s16` / `from_f32` の `channels` が 0 以下 |
| `InvalidDataLength` | `PlaybackFrame::from_s16` / `from_f32` のデータ長が `channels` の倍数でない |
| `InvalidDeviceId` | `device_id` に NUL バイトが含まれている (OS へ渡す前に拒否する) |
| `DataTooLarge(TryFromIntError)` | `PlaybackFrame::from_s16` / `from_f32` のデータ長が `i32` に収まらない |
| `InvalidAudioFormat` | WASAPI で `sample_rate` / `channels` が `WAVEFORMATEX` で表現できない (`channels` が `u16` を超える、ブロック長や毎秒バイト数が溢れる) |
| `UnknownFormat(i32)` / `UnknownDeviceType(i32)` | C 側の定数を変換するための variant。未知の値はコールバックの破棄やデバイスの除外として扱われるため、公開 API からは返らない |

バックエンド固有のエラーコード (PipeWire の `AudioPipeWireResult` など) は公開 API からは区別できず、すべて `SessionStartFailed` / `DeviceAccessDenied` などに丸められる。

### スレッド安全性

| 型 | `Send` | `Sync` |
|----|--------|--------|
| `AudioDevice` / `AudioDeviceList` | yes | yes |
| `AudioCapture` / `AudioPlayback` | yes | no |
| `AudioCaptureSession` / `AudioPlaybackSession` | yes | no |
| `ThreadGuard` | no | no |

- `AudioCapture` / `AudioPlayback` は `start` 前であれば別スレッドへ move できる
- セッションガードは `&mut AudioCapture` / `&mut AudioPlayback` を借用するため `'static` ではない。別スレッドへ move して停止する場合は `std::thread::scope` を使う

## 音声フォーマット

### キャプチャ

- すべてのバックエンドで、`sample_rate` / `channels` (0 以下は既定値) の S16 インターリーブのストリームを開き、デバイスの形式からの変換は OS / オーディオサーバーに任せる
  - CoreAudio は AudioQueue、PulseAudio / PipeWire はサーバ、WASAPI は共有モードの形式変換 (`AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY`)
- そのため、`AudioCapture::sample_rate()` / `channels()` と各 `AudioFrame` の値は設定値 (既定値を適用した値) と一致する
- 現在の実装では `AudioFrame::format` は常に `AudioFormat::S16` だが、型としては F32 もあり得るため `format` を見て `as_s16()` / `as_f32()` を使い分けるのが安全
- WASAPI で要求形式へ変換できない場合は `new` が `Error::SessionCreateFailed` を返し、デバイスの mix format へ黙ってフォールバックしない

### 再生

バックエンドがバッファに要求する形式と、ライブラリによる変換は次のとおり。

| バックエンド | バッファの形式 |
|-------------|---------------|
| CoreAudio / PulseAudio / PipeWire | 設定した `sample_rate` / `channels` の S16 |
| WASAPI | 設定した `sample_rate` / `channels` の F32 (デバイスの mix format への変換は共有モードに任せる) |

`PlaybackFrame` の形式とバッファの形式が異なる場合は次の規則で変換する。

- F32 → S16: NaN は 0、±Inf を含む値は `[-1.0, 1.0]` にクランプしてから `* 32767` を整数へ変換する
- S16 → F32: `/ 32768` する
- 同じ形式ならそのままコピーする

書き込み量の規則は次のとおり。

- 書き込むのは `min(PlaybackFrame のサンプル数, 要求サンプル数)` を `channels` の倍数に切り下げた分だけで、残りはゼロ (無音) で埋める
- 要求より多いデータは捨てられ、次回のコールバックへ持ち越されない。毎回ちょうど `frames * channels` サンプルを返すこと
- `PlaybackFrame` の `channels` / `sample_rate` / `frames` は参照されない。`data` はコールバック引数の `channels` でインターリーブされたものとして解釈し、リサンプルもチャンネル変換もしない。必ずコールバック引数と同じ `channels` / `sample_rate` で生成すること

## コールバックの契約

- コールバックはセッション開始後、バックエンドが管理するスレッド上で直列に呼ばれる。`start` を呼んだスレッドではない
- 呼び出しが直列であるため、`FnMut` の捕捉変数を `Cell` / `RefCell` / `Atomic` などを使わずに直接更新できる。境界は `Send + 'static` のみで `Sync` を要求しないため、`mpsc::Receiver` や `Cell` などの `Send + !Sync` な値も捕捉できる
- セッションガードは `'static` ではないためコールバックへ move できず、コールバック内から停止することはできない
- 実行中のコールバックはバックエンドのスレッドを占有し、`stop` もその完了を待つ。重い処理は `mpsc` などで別スレッドへ渡す
- コールバック内で panic させないこと
  - macOS / Linux ではコールバックが `extern "C"` 関数から呼ばれるため、panic するとプロセスが abort する
  - Windows ではワーカースレッドが終了し、以降コールバックは呼ばれなくなる。エラーとしては通知されない
- 1 回のコールバックで渡される・要求されるフレーム数はバックエンドとデバイスに依存し、一定とは限らない

| バックエンド | キャプチャの 1 回あたり | 再生の要求フレーム数 |
|-------------|------------------------|---------------------|
| CoreAudio | 10 ms のバッファ単位 (48000 Hz なら通常 480 フレーム) | 10 ms 固定 (`sample_rate / 100` フレーム) |
| PulseAudio | `pa_stream_peek` で得たフラグメント単位 (可変) | `pa_stream_begin_write` で得た長さ (可変) |
| PipeWire | PipeWire のバッファ単位 (quantum に依存) | `pw_buffer.requested` (無ければバッファ容量) |
| WASAPI | オーディオエンジンのパケット単位 | 10 ms のバッファの空き (`GetBufferSize - GetCurrentPadding`、可変) |

## セッションのライフサイクル

- `start()` はセッションガードを返す。ガードが生存している間は `AudioCapture` / `AudioPlayback` を可変借用するため、再度 `start` を呼ぶことも、本体を drop することもできない (コンパイルエラー)
- 停止はガードの `stop()` で行う。ガードを drop しても停止する
- `stop()` (および drop) は実行中のユーザーコールバックの完了を待ってから戻り、戻った後にそのセッションのコールバックが呼ばれることはない
- `stop()` は戻り値を持たない。停止処理の失敗は通知されない
- 停止後は同じ `AudioCapture` / `AudioPlayback` で再度 `start()` できる
- `let _ = capture.start()?;` と書くとガードが即座に drop されて停止する。`let session = ...` や `let _session = ...` で保持すること
- 実行中に設定や実際の値を参照する場合は、ガードの `config()` / `sample_rate()` / `channels()` を使う
- 開始後の非同期な障害 (サーバ切断、デバイスの取り外しなど) は通知されない。コールバックが呼ばれなくなっても、エラーとしては観測できない

## タイムスタンプ

`AudioFrame::timestamp_us` はキャプチャしたパケット先頭フレームの時刻 (マイクロ秒)。基準時刻はバックエンド固有で、壁時計ではなく、バックエンド間でも比較できない。

| バックエンド | 基準 | 単調性 |
|-------------|------|--------|
| CoreAudio | `AudioTimeStamp::mHostTime` (ホスト時刻) をマイクロ秒に変換した値 | ホスト時刻が無効なパケットは 0 になる |
| PulseAudio | `CLOCK_MONOTONIC` から `pa_stream_get_latency()` を引いた録音時刻。揺らぎを避けるため直前パケットの音声時間で予測し、50 ms を超えてずれた場合だけ補正する | セッション内で狭義単調増加 |
| PipeWire | `pw_buffer.time` (使えないビルドや 0 の場合は直前のバッファから補間、またはストリーム時刻) | セッション内で狭義単調増加 |
| WASAPI | `IAudioCaptureClient::GetBuffer` の QPC 位置。使えない場合はデバイス位置から補間し、それも無理なら読み出し時刻 | セッション内で狭義単調増加 |

- 狭義単調増加のバックエンドでは、直前以下の値になる場合に直前の値 + 1 µs にクランプする
- PulseAudio / PipeWire / WASAPI は `start` のたびに補間の状態をリセットする
- WASAPI の無音パケット (`AUDCLNT_BUFFERFLAGS_SILENT`) と PulseAudio のデータの穴はコールバックへ渡されないため、その区間はタイムスタンプが飛ぶ

## バックエンドごとの挙動差

### デバイス ID と検証のタイミング

| バックエンド | `unique_id()` / `device_id` の値 | 存在しない `device_id` の扱い |
|-------------|----------------------------------|-----------------------------|
| CoreAudio | デバイス UID (`kAudioDevicePropertyDeviceUID`) | `new` で `Error::SessionCreateFailed` |
| PulseAudio | source / sink の名前 (`pa_source_info::name` / `pa_sink_info::name`) | `new` は成功し、`start` で `Error::SessionStartFailed` |
| PipeWire | ノード名 (`node.name`)。`target.object` として渡す | 本クレートは存在を検証しない。扱いは PipeWire (セッションマネージャー) に委ねる |
| WASAPI | エンドポイント ID (`IMMDevice::GetId`) | `new` で `Error::DeviceNotFound` |

- NUL バイトを含む `device_id` は全バックエンドで `new` が `Error::InvalidDeviceId` を返す
- PulseAudio / PipeWire の `new` はサーバに接続しないため、サーバ未起動も `start` で初めて `Error::SessionStartFailed` になる

### デバイス列挙の内容

| バックエンド | 対象 | `name()` | `channels()` / `sample_rate()` |
|-------------|------|----------|-------------------------------|
| CoreAudio | 入力 / 出力ストリームを持つデバイス。両方を持つデバイスは `Input` と `Output` の 2 件になり、`unique_id()` は同じ値 | デバイス名 | スコープごとのチャンネル数 (0 なら 2)、公称サンプルレート (0 なら 48000) |
| PulseAudio | source (monitor source は除外) と sink | `description` (無ければ名前) | `sample_spec` の値 (0 なら source は 1 / sink は 2、48000) |
| PipeWire | `media.class` が `Audio/Source` / `Audio/Sink` のノード | `node.description` (無ければ `node.name`) | `audio.channels` / `audio.rate` プロパティ (無ければ 1 / 48000) |
| WASAPI | `DEVICE_STATE_ACTIVE` のキャプチャ / レンダーエンドポイント | `PKEY_Device_FriendlyName` (取得できなければ `Unknown Device`) | `GetMixFormat` の値 (取得できなければ 2 / 48000) |

- 列挙順はバックエンドに依存する
- CoreAudio で 1 つのデバイスが入出力の両方を持つ場合、同じ `unique_id()` が `Input` と `Output` に現れる。USB オーディオのように入力と出力が別の CoreAudio デバイスとして公開される場合は、名前が同じでも `unique_id()` は異なる

### ブロッキングとタイムアウト

- PulseAudio の `start` はサーバが応答しない場合、無期限にブロックする
- PulseAudio / PipeWire のデバイス列挙はタイムアウトを持たない
- PipeWire の `start` はストリームが STREAMING / PAUSED にならないまま約 10 秒 (待機の粒度により最大約 11 秒) 経過すると `Error::SessionStartFailed` を返す
  - 経過時間は `CLOCK_MONOTONIC` で判定するが、待機には `CLOCK_REALTIME` を使う。システム時刻の後退やサスペンドがある環境ではこの上限は保証されない
- PipeWire でストリームが PAUSED になった場合は、activate して開始成功として扱う

### macOS のマイクアクセス

macOS のキャプチャにはマイクへのアクセス許可 (プライバシー設定) が必要。本クレートは許可の要求や状態確認を行わない。

## コード例

### デバイス列挙

```rust
use shiguredo_audio_device::{AudioDeviceList, AudioDeviceType};

fn main() -> shiguredo_audio_device::Result<()> {
    // Windows では COM を MTA で初期化する。ガードはスレッドの終了まで保持する
    let _thread_guard = shiguredo_audio_device::initialize_thread()?;

    let devices = AudioDeviceList::enumerate()?;
    for device in &devices {
        let device_type = match device.device_type() {
            AudioDeviceType::Input => "input",
            AudioDeviceType::Output => "output",
        };
        println!(
            "{device_type}: {} (id: {}) {}ch {}Hz",
            device.name()?,
            device.unique_id()?,
            device.channels(),
            device.sample_rate()
        );
    }
    Ok(())
}
```

### デバイスを指定してキャプチャし、別スレッドで処理する

```rust
use std::sync::mpsc;
use std::time::Duration;

use shiguredo_audio_device::{AudioCapture, AudioCaptureConfig, AudioDeviceList, AudioFrameOwned};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _thread_guard = shiguredo_audio_device::initialize_thread()?;

    // 入力デバイスの先頭を使う。見つからなければ None (デフォルトデバイス) にする
    let input_devices = AudioDeviceList::enumerate_input()?;
    let device_id = match input_devices.as_slice().first() {
        Some(device) => Some(device.unique_id()?),
        None => None,
    };

    let (sender, receiver) = mpsc::channel::<AudioFrameOwned>();
    let mut capture = AudioCapture::new(
        AudioCaptureConfig {
            device_id,
            sample_rate: 48000,
            channels: 1,
        },
        move |frame| {
            // frame.data はコールバックの実行中だけ有効なので、所有データにして送る。
            // 受信側が終了している場合の送信エラーは無視する
            let _ = sender.send(frame.to_owned());
        },
    )?;

    let session = capture.start()?;
    let mut captured_frames = 0i64;
    // 3 秒分を受け取るまで処理する
    while captured_frames < i64::from(session.sample_rate()) * 3 {
        let frame = receiver.recv_timeout(Duration::from_secs(5))?;
        if let Some(samples) = frame.as_s16() {
            let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
            println!("timestamp={}us frames={} peak={peak}", frame.timestamp_us, frame.frames);
        }
        captured_frames += i64::from(frame.frames);
    }
    session.stop();
    Ok(())
}
```

### サイン波の再生

```rust
use std::f64::consts::PI;
use std::time::Duration;

use shiguredo_audio_device::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

fn main() -> shiguredo_audio_device::Result<()> {
    let _thread_guard = shiguredo_audio_device::initialize_thread()?;

    // コールバックは直列に呼ばれるため、再生位置を直接更新してよい
    let mut position = 0u64;
    let mut playback = AudioPlayback::new(
        AudioPlaybackConfig::default(),
        move |frames, channels, sample_rate| {
            let frames = frames as usize;
            let channels = channels as usize;
            let mut samples = vec![0i16; frames * channels];
            for index in 0..frames {
                let t = (position + index as u64) as f64 / f64::from(sample_rate);
                let value = ((2.0 * PI * 440.0 * t).sin() * 0.5 * f64::from(i16::MAX)) as i16;
                for channel in 0..channels {
                    samples[index * channels + channel] = value;
                }
            }
            position += frames as u64;
            // 要求と同じ channels / sample_rate で生成する
            PlaybackFrame::from_s16(&samples, channels as i32, sample_rate).ok()
        },
    )?;

    let session = playback.start()?;
    std::thread::sleep(Duration::from_secs(3));
    session.stop();
    Ok(())
}
```

### 別スレッドから PCM を供給して再生する

```rust
use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Duration;

use shiguredo_audio_device::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

fn main() -> shiguredo_audio_device::Result<()> {
    let _thread_guard = shiguredo_audio_device::initialize_thread()?;

    // 供給側は再生セッションの sample_rate / channels でインターリーブした PCM を送る
    let (sender, receiver) = mpsc::channel::<Vec<i16>>();
    // mpsc::Receiver は Send + !Sync だが、コールバックの境界は Send だけなので捕捉できる
    let mut pending = VecDeque::new();
    let mut playback = AudioPlayback::new(
        AudioPlaybackConfig {
            device_id: None,
            sample_rate: 48000,
            channels: 2,
        },
        move |frames, channels, sample_rate| {
            while let Ok(chunk) = receiver.try_recv() {
                pending.extend(chunk);
            }
            // 要求されたサンプル数だけ取り出し、不足分は無音で埋める。
            // 余分に返したデータは捨てられるため、ちょうど要求分を返す
            let needed = frames as usize * channels as usize;
            let available = needed.min(pending.len());
            let mut samples: Vec<i16> = pending.drain(..available).collect();
            samples.resize(needed, 0);
            PlaybackFrame::from_s16(&samples, channels, sample_rate).ok()
        },
    )?;

    let session = playback.start()?;
    for _ in 0..100 {
        // 10 ms 分 (48000 Hz / 2 ch) の無音を送る例
        let _ = sender.send(vec![0i16; 480 * 2]);
        std::thread::sleep(Duration::from_millis(10));
    }
    session.stop();
    Ok(())
}
```

### Linux でバックエンドを明示的に選ぶ

`pulse` と `pipewire` の両方の feature を有効にしている場合の例。

```rust
use shiguredo_audio_device::{AudioCapture, AudioCaptureConfig, AudioDeviceList};

fn open_capture(use_pipewire: bool) -> shiguredo_audio_device::Result<AudioCapture> {
    if use_pipewire {
        // デバイス ID は同じバックエンドで列挙した unique_id() を使う
        let devices = AudioDeviceList::enumerate_input_pipewire()?;
        let device_id = match devices.as_slice().first() {
            Some(device) => Some(device.unique_id()?),
            None => None,
        };
        let config = AudioCaptureConfig {
            device_id,
            ..AudioCaptureConfig::default()
        };
        AudioCapture::new_pipewire(config, |_frame| {})
    } else {
        AudioCapture::new_pulse(AudioCaptureConfig::default(), |_frame| {})
    }
}
```

デバイス ID の形式はバックエンドごとに異なるため、列挙と構築には同じバックエンドを使うこと。

### 別スレッドからセッションを停止する

```rust
use std::time::Duration;

use shiguredo_audio_device::{AudioCapture, AudioCaptureConfig};

fn main() -> shiguredo_audio_device::Result<()> {
    let _thread_guard = shiguredo_audio_device::initialize_thread()?;

    let mut capture = AudioCapture::new(AudioCaptureConfig::default(), |_frame| {})?;
    let session = capture.start()?;

    // ガードは capture を借用していて 'static ではないため、scope 付きスレッドへ move する
    std::thread::scope(|scope| {
        scope.spawn(move || {
            // 別スレッドでもライブラリを使う前に初期化する
            let _thread_guard = shiguredo_audio_device::initialize_thread()
                .expect("failed to initialize the audio library on the stop thread");
            std::thread::sleep(Duration::from_secs(3));
            session.stop();
        });
    });

    // 停止後は同じ capture で再開始できる
    let session = capture.start()?;
    std::thread::sleep(Duration::from_secs(1));
    drop(session);
    Ok(())
}
```

## 既知の制限事項

- **キャプチャは S16 のみ**: 全バックエンドで S16 のストリームを開く。F32 でキャプチャする設定はない
- **再生フレームの形式の不一致を検出しない**: 再生コールバックが返す `PlaybackFrame` の `channels` / `sample_rate` は無視され、要求と異なっていてもエラーにも無音にもならず、ライブラリ側で変換もしない
- **`channels` / `sample_rate` の上限検証がない (macOS / Linux)**: 設定値はそのまま C 側へ渡る
  - PulseAudio のチャンネル数上限 (32) を超える値なども拒否されないため、デバイスとサーバが扱える現実的な値を指定すること
  - WASAPI は `WAVEFORMATEX` で表現できない値を `Error::InvalidAudioFormat` で拒否する
- **無音・データの穴の扱いがバックエンドで異なる**: CoreAudio は無音もそのまま渡し、WASAPI の無音パケットと PulseAudio の穴はコールバックへ渡さない
- **CoreAudio のタイムスタンプ 0**: ホスト時刻が無効なパケットでは `timestamp_us` が 0 になり、単調増加が崩れる
- **エラー原因の区別**: バックエンド固有のエラーコードは公開されず、サーバ未起動・タイムアウト・権限不足などを区別できない
- **非同期障害の通知なし**: 開始後のサーバ切断やデバイス取り外しは通知されない
- **PulseAudio の無期限ブロック**: `start` とデバイス列挙はサーバが応答しない場合に戻らない
- **デフォルトデバイスの問い合わせ API なし**: `device_id: None` でデフォルトデバイスを使えるが、どのデバイスがデフォルトかは取得できない
- **共有モードのみ (Windows)**: WASAPI の排他モードは使えない
- **macOS のマイク許可**: 許可の要求や状態確認の API はない

## サンプル

| サンプル | 内容 |
|---------|------|
| `device_list` | デバイス一覧を JSON で出力する |
| `device_info` | デバイスごとの詳細情報と統計情報 (入出力の件数、サンプルレートとチャンネル数の一覧) を JSON で出力する |
| `playback_sine` | デフォルト出力デバイスに 440 Hz のサイン波を S16 で再生し、Enter キーで停止する |

```bash
cargo run --example device_list
cargo run --example device_info
cargo run --example playback_sine

# Linux で PipeWire をデフォルトバックエンドにして実行する
cargo run --example device_list --no-default-features --features pipewire,default-pipewire
```

バックエンドが 1 つも有効でない場合、各サンプルはエラーメッセージを表示して終了する。

## モジュール構成

| モジュール / コンポーネント | 役割 |
|---------------------------|------|
| クレートルート | 公開型の再公開 |
| `runtime` | `initialize_thread()` と `ThreadGuard` |
| `error` | `Error` と `Result` |
| `common` | `AudioDeviceType`、`AudioFormat`、`AudioFrame`、`AudioFrameOwned`、`AudioCaptureConfig`、`AudioPlaybackConfig`、`PlaybackFrame` と再生用のフォーマット変換 |
| `device` | `AudioDevice` と `AudioDeviceList` (バックエンドへの振り分け) |
| `capture` | `AudioCapture` と `AudioCaptureSession` (バックエンドへの振り分け) |
| `playback` | `AudioPlayback` と `AudioPlaybackSession` (バックエンドへの振り分け) |
| `device_ffi` / `capture_ffi` / `playback_ffi` | macOS / Linux 共通の FFI 実装。バックエンドごとの C 関数テーブルを切り替える |
| `ffi` | `bindgen` で生成したバインディングの取り込み |
| `device_wasapi` / `capture_wasapi` / `playback_wasapi` | Windows (WASAPI) 実装。COM の初期化ガード、`WAVEFORMATEX` の構築、ワーカースレッド |
| C セッション API の共通ヘッダー | create / start / stop / destroy とコールバックの契約 |
| CoreAudio バックエンド | AudioQueue を使う Objective-C 実装 |
| PulseAudio バックエンド | `pa_threaded_mainloop` を使う C 実装 |
| PipeWire バックエンド | `pw_thread_loop` を使う C 実装 |
| ビルドスクリプト | feature から `enable_*` cfg を設定し、C / Objective-C のビルドと `bindgen` を行う |
