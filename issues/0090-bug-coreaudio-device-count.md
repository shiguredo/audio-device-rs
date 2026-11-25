# CoreAudio の列挙が再取得後のデバイス数を使わず未初期化 ID を読む

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-coreaudio-device-count
- Polished: {YYYY-MM-DD}

## 目的

`audio_coreaudio_enumerate_devices` と `find_device_by_uid` が、
`AudioObjectGetPropertyDataSize` で得た `deviceCount` を
`AudioObjectGetPropertyData` の後もそのまま使う。サイズ取得とデータ取得の
間にデバイスが減ると、配列の末尾は未初期化のまま残り、その値を
`AudioDeviceID` として使う。ホットプラグ時に未初期化値を読まないようにする。

## 現状

`audio_coreaudio_enumerate_devices` は `dataSize` から
`deviceCount = dataSize / sizeof(AudioDeviceID)` を求め、`malloc(dataSize)` の
領域へ `AudioObjectGetPropertyData` を呼ぶ。その後のループは、取得前の
`deviceCount` を使う。

`find_device_by_uid` も同じ順序で `deviceCount` を求め、UID の比較ループに
取得前の件数を使う。

macOS SDK の `AudioHardware.h` は `AudioObjectGetPropertyData` の `ioDataSize`
について「入力時はバッファのサイズ、終了時はバッファのうち実際に使われた量を
示す」と定義している。デバイス数が減少した場合、`dataSize` は更新されるが
`deviceCount` は古いままである。配列の未初期化領域を `AudioObjectGetPropertyDataSize`
や UID 取得の `AudioDeviceID` として使う。

## 設計方針

- `AudioObjectGetPropertyData` の成功後に
  `deviceCount = dataSize / sizeof(AudioDeviceID)` を再計算する
- 再計算した `deviceCount` だけをループする
- `dataSize` が `sizeof(AudioDeviceID)` の倍数でない場合や、取得後の
  `dataSize` が確保したサイズを超える場合は、その列挙をエラーにする
- 取得後の件数が 0 の場合は、空のリストとして扱う
- `audio_coreaudio_enumerate_devices` と `find_device_by_uid` の両方に
  同じ修正を適用する
- 必要なら `calloc` で配列をゼロ初期化する。ただし、再計算した件数だけを
  使うことを主な対策とし、未初期化値の読み出しに依存しない

## 完了条件

- `AudioObjectGetPropertyData` の後に再計算した `deviceCount` だけを
  ループしている
- `dataSize` が `sizeof(AudioDeviceID)` の倍数でない場合と、取得後の
  `dataSize` が確保サイズを超える場合に、列挙がエラーになる
- `audio_coreaudio_enumerate_devices` と `find_device_by_uid` の両方で、
  未初期化の `AudioDeviceID` を読まない
- ホットプラグを再現できる macOS 実機テスト、またはコードレビューで
  サイズ取得とデータ取得の間の件数変化を確認する
