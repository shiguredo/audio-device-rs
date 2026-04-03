# Linux で USB オーディオデバイスを認識させる

Linux で USB オーディオデバイスを audio-device-rs から認識するために必要な手順をまとめる。

## 前提パッケージ

- `snd_usb_audio` カーネルモジュール (通常はデフォルトで有効)
- PulseAudio 利用時: `libpulse-dev`
- PipeWire 利用時: `libpipewire-0.3-dev`, `pipewire-alsa`
  - `pipewire-alsa` がないと PipeWire が ALSA デバイスを認識しない

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

# PulseAudio/PipeWire レベルの確認
pactl list sources
pactl list sinks
```

## はまりどころ

### USB デバイスが ALSA に認識されない

`snd_usb_audio` がロード済みでもデバイスにバインドされないことがある。
`dmesg` で確認し、必要なら unbind/bind で再認識させる。

```bash
# デバイスのインターフェース番号は lsusb -t で確認
lsusb -t

# unbind/bind で再認識
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

グループ変更後はログアウト/再ログインまたは再起動が必要。

### PipeWire が ALSA デバイスを認識しない

`pipewire-alsa` パッケージが必要。

```bash
sudo apt-get install -y pipewire-alsa
```

インストール後、PipeWire の再起動またはシステム再起動が必要。

### USB full-speed 接続

USB Audio Class デバイスが full-speed (12 Mbps) で接続されることがある。
`dmesg` に `new full-speed USB device` と表示される場合、ハブを介さず直結を試す。
デバイスによっては full-speed でも正常に動作する。

## GitHub Actions self-hosted runner での注意

- `rustup` / `cargo` が runner の PATH から見えない場合がある
- `$GITHUB_PATH` に `$HOME/.cargo/bin` を追加する必要がある
- ALSA / PipeWire の認識にはシステム再起動が必要な場合がある
