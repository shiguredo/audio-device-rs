# audio-device-rs

[![crates.io](https://img.shields.io/crates/v/shiguredo_audio_device.svg)](https://crates.io/crates/shiguredo_audio_device)
[![docs.rs](https://docs.rs/shiguredo_audio_device/badge.svg)](https://docs.rs/shiguredo_audio_device)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![GitHub Actions](https://github.com/shiguredo/audio-device-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/shiguredo/audio-device-rs/actions/workflows/ci.yml)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white)](https://discord.gg/shiguredo)

## About Shiguredo's open source software

We will not respond to PRs or issues that have not been discussed on Discord. Also, Discord is only available in Japanese.

Please read <https://github.com/shiguredo/oss> before use.

## 時雨堂のオープンソースソフトウェアについて

利用前に <https://github.com/shiguredo/oss> をお読みください。

## 概要

macOS / Linux / Windows に対応したオーディオデバイスライブラリです。
音声キャプチャ (マイク入力) と音声再生 (スピーカー出力) を提供します。

## 特徴

- macOS / Linux / Windows で共通のキャプチャ / 再生 API
- Linux では PulseAudio と PipeWire を同時に有効化し、実行時にバックエンドを選択可能
- ランタイム依存クレートなし (システムライブラリのみ)

## 対応プラットフォーム

- macOS: CoreAudio / AudioToolbox
- Linux: PulseAudio (デフォルト) / PipeWire
- Windows: WASAPI

## feature

デフォルト feature は次のとおりです。

- macOS: `coreaudio` (`default-coreaudio`)
- Linux: `pulse` (`default-pulse`)
- Windows: `wasapi` (`default-wasapi`)

Linux では `pulse` と `pipewire` を同時に有効化できます。
実行時に使うバックエンドは、`AudioDeviceList::enumerate_pulse()` / `AudioCapture::new_pipewire()` / `AudioPlayback::new_pipewire()` のように明示 API で選べます。

`default-*` feature はプラットフォームごとに 1 つだけ有効にしてください。
`AudioDeviceList::enumerate()` や `AudioCapture::new()` / `AudioPlayback::new()` は、有効な `default-*` に対応するバックエンドを使います。

## ビルド要件

### 共通

- Rust 1.93 以降

### macOS

追加の依存パッケージは不要です。

### Ubuntu (Linux)

PulseAudio バックエンド (デフォルト):

```bash
sudo apt install libpulse-dev
```

最近の Ubuntu (22.04 以降) では PipeWire がデフォルトのオーディオサーバーになっています。PulseAudio バックエンドを使用する場合は `pipewire-pulse` (PulseAudio 互換レイヤー) が動作している必要があります。

```bash
sudo apt install pipewire-pulse
systemctl --user enable --now pipewire pipewire-pulse
```

PipeWire バックエンド:

```bash
sudo apt install libpipewire-0.3-dev pipewire-alsa
```

`pipewire-alsa` がないと PipeWire が ALSA デバイスを認識しません。
PipeWire デーモンが動作している必要があります。

```bash
systemctl --user enable --now pipewire
```

両方を有効にする場合は、上記の開発パッケージをそれぞれインストールしてください。

Linux で USB オーディオデバイスを認識させる手順は [docs/LINUX.md](docs/LINUX.md) を参照してください。

### Windows

追加の依存パッケージは不要です。

## ビルド

```bash
# デフォルト (macOS CoreAudio / Linux PulseAudio / Windows WASAPI)
cargo build -p shiguredo_audio_device

# Linux: PipeWire をデフォルトバックエンドにする
cargo build -p shiguredo_audio_device --no-default-features --features pipewire,default-pipewire

# Linux: PulseAudio と PipeWire を共存させ、デフォルトは PulseAudio のままにする
cargo build -p shiguredo_audio_device --features pipewire
```

## 使い方

### デバイス列挙

```rust
use shiguredo_audio_device::{AudioDeviceList, AudioDeviceType};

// 全デバイス (入力 / 出力) を取得
let device_list = AudioDeviceList::enumerate()?;
for device in &device_list {
    let device_type = match device.device_type() {
        AudioDeviceType::Input => "入力",
        AudioDeviceType::Output => "出力",
    };
    println!(
        "{}: {} (ID: {}) {}ch {}Hz",
        device_type,
        device.name()?,
        device.unique_id()?,
        device.channels(),
        device.sample_rate()
    );
}

// 入力デバイスのみ取得
let input_devices = AudioDeviceList::enumerate_input()?;

// 出力デバイスのみ取得
let output_devices = AudioDeviceList::enumerate_output()?;
```

Linux で複数バックエンドを有効にしている場合は、明示 API も使えます。

```rust
let pulse_devices = AudioDeviceList::enumerate_pulse()?;
let pipewire_devices = AudioDeviceList::enumerate_pipewire()?;
```

### キャプチャ

```rust
use shiguredo_audio_device::{AudioCapture, AudioCaptureConfig};

// キャプチャ設定
let config = AudioCaptureConfig {
    device_id: None, // デフォルトデバイスを使用
    sample_rate: 48000,
    channels: 1,
};

// コールバックでフレームを受信
let mut capture = AudioCapture::new(config, |frame| {
    println!(
        "フレーム: {}frames {}ch {}Hz timestamp={}us",
        frame.frames, frame.channels,
        frame.sample_rate, frame.timestamp_us
    );
})?;

// キャプチャ開始
capture.start()?;

// ... キャプチャ中 ...

// キャプチャ停止
capture.stop();
```

### 再生

```rust
use shiguredo_audio_device::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

let config = AudioPlaybackConfig {
    device_id: None, // デフォルトデバイスを使用
    sample_rate: 48000,
    channels: 2,
};

// コールバックは (要求フレーム数, チャンネル数, サンプルレート) を受け取り、
// PlaybackFrame を返す。None を返すと無音になる
let mut playback = AudioPlayback::new(config, |frames, channels, sample_rate| {
    let total = (frames * channels) as usize;
    let samples = vec![0i16; total]; // ここで PCM を生成する
    PlaybackFrame::from_s16(&samples, channels, sample_rate).ok()
})?;

playback.start()?;

// ... 再生中 ...

playback.stop();
```

## サンプル

`examples/` 以下に実行可能なサンプルがあります。

```bash
# デバイス一覧を JSON で出力する
cargo run --example device_list

# デバイスごとの詳細情報を JSON で出力する
cargo run --example device_info

# デフォルト出力デバイスに 440Hz のサイン波を再生する
cargo run --example playback_sine
```

## ライセンス

Apache License 2.0

```text
Copyright 2026-2026, Shiguredo Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
