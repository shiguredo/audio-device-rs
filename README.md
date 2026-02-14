# audio-device-rs

[![shiguredo_audio_device](https://img.shields.io/crates/v/shiguredo_audio_device.svg)](https://crates.io/crates/shiguredo_audio_device)
[![Documentation](https://docs.rs/shiguredo_audio_device/badge.svg)](https://docs.rs/shiguredo_audio_device)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

> [!WARNING]
> このライブラリは開発中であり、仕様が積極的に変更される場合があります。

## About Shiguredo's open source software

We will not respond to PRs or issues that have not been discussed on Discord. Also, Discord is only available in Japanese.

Please read <https://github.com/shiguredo/oss> before use.

## 時雨堂のオープンソースソフトウェアについて

利用前に <https://github.com/shiguredo/oss> をお読みください。

## 概要

macOS / Linux / Windows に対応したオーディオデバイスライブラリです。

## 対応プラットフォーム

- macOS: CoreAudio / AudioToolbox
- Linux: PulseAudio (デフォルト) / PipeWire
- Windows: WASAPI

## Linux の feature

Linux では `pulse` (デフォルト) と `pipewire` の 2 つの feature を選択できます。両方を同時に指定することはできません。

## ビルド要件

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
sudo apt install libpipewire-0.3-dev
```

PipeWire デーモンが動作している必要があります。

```bash
systemctl --user enable --now pipewire
```

### Windows

追加の依存パッケージは不要です。

## ビルド

```bash
# デフォルト (macOS / Linux PulseAudio / Windows)
cargo build -p shiguredo_audio_device

# Linux PipeWire バックエンド
cargo build -p shiguredo_audio_device --no-default-features --features pipewire
```

## 使い方

### デバイス列挙

```rust
use shiguredo_audio_device::{AudioDeviceList, AudioDeviceType};

// 全デバイス（入力・出力）を取得
let device_list = AudioDeviceList::enumerate()?;
for device in device_list.devices() {
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
