# Linux で USB オーディオデバイスを認識させる

Linux で USB オーディオデバイスを audio-device-rs から認識するために必要な手順をまとめる。
デバイス列挙、音声キャプチャ、音声再生のいずれでも、先に OS / オーディオサーバー側でデバイスが見えている必要がある。

ビルド方法や feature の詳細は [README.md](../README.md) を参照すること。

## バックエンドと feature

Linux では次の 2 つのバックエンドを使える。

- PulseAudio (`pulse` / デフォルトは `default-pulse`)
- PipeWire (`pipewire` / `default-pipewire`)

`pulse` と `pipewire` は同時に有効化できる。
実行時に使うバックエンドは `AudioDeviceList::enumerate_pulse()` / `AudioDeviceList::enumerate_pipewire()` のように明示 API で選べる。

```bash
# デフォルト (PulseAudio)
cargo build -p shiguredo_audio_device

# PipeWire をデフォルトバックエンドにする
cargo build -p shiguredo_audio_device --no-default-features --features pipewire,default-pipewire

# PulseAudio と PipeWire を共存させ、デフォルトは PulseAudio のままにする
cargo build -p shiguredo_audio_device --features pipewire
```

`default-*` feature はプラットフォームごとに 1 つだけ有効にすること。

## 前提パッケージ

- `snd_usb_audio` カーネルモジュール (通常はデフォルトで有効)
- PulseAudio 利用時: `libpulse-dev`
- PipeWire 利用時: `libpipewire-0.3-dev`, `pipewire-alsa`
  - `pipewire-alsa` がないと PipeWire が ALSA デバイスを認識しない

最近の Ubuntu (22.04 以降) では PipeWire がデフォルトのオーディオサーバーになっている。
PulseAudio バックエンドを使う場合は、`pipewire-pulse` (PulseAudio 互換レイヤー) が動作している必要がある。

```bash
# PulseAudio バックエンド用
sudo apt install libpulse-dev pipewire-pulse
systemctl --user enable --now pipewire pipewire-pulse

# PipeWire バックエンド用
sudo apt install libpipewire-0.3-dev pipewire-alsa
systemctl --user enable --now pipewire

# 両方を有効にする場合は、上記をまとめて入れる
sudo apt install libpulse-dev pipewire-pulse libpipewire-0.3-dev pipewire-alsa
systemctl --user enable --now pipewire pipewire-pulse
```

## USB オーディオデバイスの認識確認

```bash
# デバイスの物理接続確認
lsusb

# ALSA レベルの認識確認
arecord -l
aplay -l

# サウンドカード確認
cat /proc/asound/cards

# カーネルレベルの認識確認
dmesg | grep -i audio

# PulseAudio / PipeWire レベルの確認 (pipewire-pulse 経由でも pactl が使える)
pactl list sources
pactl list sinks
```

audio-device-rs 側からも確認できる。

```bash
# デフォルトバックエンド (PulseAudio) でデバイス一覧を出力する
cargo run --example device_list

# PipeWire バックエンドでデバイス一覧を出力する
cargo run --example device_list --no-default-features --features pipewire,default-pipewire

# デフォルト出力デバイスへサイン波を再生して動作確認する
cargo run --example playback_sine
```

## はまりどころ

### USB デバイスが ALSA に認識されない

`snd_usb_audio` がロード済みでもデバイスにバインドされないことがある。
`dmesg` で確認し、必要なら unbind / bind で再認識させる。

```bash
# デバイスのインターフェース番号は lsusb -t で確認
lsusb -t

# unbind / bind で再認識
echo -n "1-1:1.0" | sudo tee /sys/bus/usb/drivers/snd-usb-audio/unbind
echo -n "1-1:1.0" | sudo tee /sys/bus/usb/drivers/snd-usb-audio/bind
```

### ユーザーが audio グループに属していない

```bash
# 確認
groups

# 追加
sudo usermod -aG audio <username>
```

グループ変更後はログアウト / 再ログイン、または再起動が必要。

### PipeWire が ALSA デバイスを認識しない

`pipewire-alsa` パッケージが必要。

```bash
sudo apt install pipewire-alsa
```

インストール後、PipeWire の再起動またはシステム再起動が必要。

### PulseAudio バックエンドでデバイスが見えない

最近の Ubuntu では `pipewire-pulse` が止まっていると、PulseAudio バックエンドからデバイスが見えないことがある。

```bash
systemctl --user status pipewire-pulse
systemctl --user enable --now pipewire pipewire-pulse
```

### USB full-speed 接続

USB Audio Class デバイスが full-speed (12 Mbps) で接続されることがある。
`dmesg` に `new full-speed USB device` と表示される場合、ハブを介さず直結を試す。
デバイスによっては full-speed でも正常に動作する。

## GitHub Actions self-hosted runner での注意

- `rustup` / `cargo` が runner の PATH から見えない場合がある
- `$GITHUB_PATH` に `$HOME/.cargo/bin` を追加する必要がある
- ALSA / PipeWire の認識にはシステム再起動が必要な場合がある
