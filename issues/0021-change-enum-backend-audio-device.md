# AudioDevice, AudioDeviceList, AudioCapture, AudioPlayback を enum newtype 化し、pulse と pipewire を共存可能にする

Created: 2026-06-23
Model: deepseek v4-pro
Polished: 2026-06-23

## 目的

同一バイナリで複数のオーディオバックエンド （CoreAudio, PulseAudio, PipeWire, WASAPI） を実行時に選択可能にし、かつバックエンド間で型レベルのインターフェース保証を得る。現在の `#[cfg(target_os)]` による条件コンパイルと feature flag の相互排他制約を撤廃し、全バックエンドを enum newtype で統合する。

## 優先度根拠

- PulseAudio と PipeWire が両方動作する環境 （多くの Linux ディストリビューション） で、ユーザーがどちらを使うか選択できないのは実用上の致命的な制約である
- `#[cfg(target_os)]` による条件コンパイルは、片方のプラットフォームへの修正漏れをコンパイル時に検出できない。Windows と UNIX で分岐する現状では、一方の修正が他方に波及しない品質リスクがある
- FFI 実装が単一の bindgen 出力を共有しているため、現状のままバックエンドを増やすとシンボル衝突が避けられない

## 現状

### 1. pulse と pipewire が共存できない

`build.rs` の以下のコードが両 feature の同時有効化を拒否している:

```rust
// build.rs:20-22
if has_pulse && has_pipewire {
    panic!("features \"pulse\" and \"pipewire\" are mutually exclusive");
}
```

### 2. 同一インターフェースの型レベルの保証がない

各プラットフォーム/バックエンド （macOS/CoreAudio, Windows/WASAPI, PulseAudio, PipeWire） は同じシグネチャのメソッドを持つが、`#[cfg]` による条件コンパイルで切り替えているだけで、trait や enum による型レベルの保証がない。片方のバックエンドにメソッドを追加してもう片方に追加し忘れてもコンパイルが通ってしまう。

### 3. FFI バックエンドが単一の bindgen 出力を共有している

`src/ffi.rs:6` の `include!(concat!(env!("OUT_DIR"), "/bindings.rs"));` が単一の bindgen 出力であり、全 FFI バックエンド （macOS, PulseAudio, PipeWire） で C 関数名が同一のためシンボルが衝突する根本原因になっている。`build.rs:91-109` の `generate_bindings()` は `audio_c.h` に対して単一の bindgen 実行しか行っていない。

### 4. 現状のファイル構成

| ファイル | 役割 | プラットフォーム |
|---|---|---|
| `src/lib.rs` | クレートルート | 共通 |
| `src/common.rs` | 共通型定義、`CaptureContext` | 共通 |
| `src/error.rs` | `Error` 列挙型 | 共通 |
| `src/device.rs` | `AudioDevice`, `AudioDeviceList`, `from_ffi` | macOS/Linux |
| `src/device_windows.rs` | `AudioDevice`, `AudioDeviceList`, `SendHandle`, `SendPtr`, `init_com_mta`, `determine_audio_format`, `get_device_by_id` | Windows |
| `src/capture.rs` | `AudioCapture`, `frame_callback` | macOS/Linux |
| `src/capture_windows.rs` | `AudioCapture`, `SessionData`, `capture_thread_func` | Windows |
| `src/playback.rs` | `AudioPlayback` 未実装スタブ | macOS/Linux |
| `src/playback_windows.rs` | `AudioPlayback`, `SessionData`, `PlaybackContext` | Windows |
| `src/ffi.rs` | 単一 bindgen include | macOS/Linux |
| `src/audio_c.h` | 全 FFI バックエンド共通 C ヘッダ | macOS/Linux |
| `src/audio_c.m` | macOS CoreAudio 実装 | macOS |
| `src/audio_pulse.c` | PulseAudio 実装 | Linux |
| `src/audio_pipewire.c` | PipeWire 実装 | Linux |
| `build.rs` | コンパイル・bindgen | 共通 |
| `Cargo.toml` | feature flag: `pulse`, `pipewire` | 共通 |

## 設計方針

video-device-rs の 0019 と同様に、公開型を enum newtype ラッパーとして実装し、内部でプラットフォーム/バックエンド別の具象型を保持する方式を採用する。

- enum のバリアント一致チェックでコンパイル時の安全性を確保
- trait や GAT を使わないため `dyn` による動的ディスパッチの制約がない
- `AudioCapture::new()` 等のコンストラクタを enum 側の関連関数として提供することで、利用者の `#[cfg]` 分岐を削減
- バックエンド別の feature flag 相互排他制約を撤廃し、同一バイナリで両バックエンドの共存を可能にする

### enum とバックエンドの対応

公開型は newtype struct で包み、内部 enum に `#[cfg]` でバリアントを条件付与する:

| 公開型 | 内部 enum | バリアント |
|---|---|---|
| `AudioDevice` | `AudioDeviceInner` | `#[cfg(any(enable_ca, enable_pulse, enable_pipewire))] Ffi(FfiDeviceImpl)` / `#[cfg(enable_wasapi)] Wasapi(WasapiDeviceImpl)` |
| `AudioDeviceList` | `AudioDeviceListInner` | `#[cfg(any(enable_ca, enable_pulse, enable_pipewire))] Ffi(FfiDeviceListImpl)` / `#[cfg(enable_wasapi)] Wasapi(WasapiDeviceListImpl)` |
| `AudioCapture` | `AudioCaptureInner` | `#[cfg(any(enable_ca, enable_pulse, enable_pipewire))] Ffi(FfiCaptureImpl)` / `#[cfg(enable_wasapi)] Wasapi(WasapiCaptureImpl)` |
| `AudioPlayback` | `AudioPlaybackInner` | `#[cfg(any(enable_ca, enable_pulse, enable_pipewire))] Ffi(FfiPlaybackImpl)` / `#[cfg(enable_wasapi)] Wasapi(WasapiPlaybackImpl)` |

全バックエンドが無効な場合に enum のバリアントが 0 個になるのを防ぐため、`src/lib.rs` の先頭でコンパイルエラーを出す:

```rust
#[cfg(not(any(enable_ca, enable_pulse, enable_pipewire, enable_wasapi)))]
compile_error!("No audio backend is enabled for this target. Enable at least one of: ca, pulse, pipewire, wasapi");
```

### enum newtype の委譲パターン

公開型の各メソッドは内部の具象型に `match` で委譲する。`#[cfg]` でガードされたバリアントに対応するため、各アームにも `#[cfg]` を付与する:

```rust
// src/device.rs (新規)
pub struct AudioDevice(AudioDeviceInner);

enum AudioDeviceInner {
    #[cfg(any(enable_ca, enable_pulse, enable_pipewire))]
    Ffi(FfiDeviceImpl),
    #[cfg(enable_wasapi)]
    Wasapi(WasapiDeviceImpl),
}

impl AudioDevice {
    pub fn name(&self) -> Result<String> {
        match &self.0 {
            #[cfg(any(enable_ca, enable_pulse, enable_pipewire))]
            AudioDeviceInner::Ffi(d) => d.name(),
            #[cfg(enable_wasapi)]
            AudioDeviceInner::Wasapi(d) => d.name(),
        }
    }
    // unique_id, channels, sample_rate, device_type も同様
}

impl AudioDeviceList {
    pub fn devices(&self) -> &[AudioDevice] {
        match &self.0 {
            #[cfg(any(enable_ca, enable_pulse, enable_pipewire))]
            AudioDeviceListInner::Ffi(d) => d.devices(),
            #[cfg(enable_wasapi)]
            AudioDeviceListInner::Wasapi(d) => d.devices(),
        }
    }
    // len, is_empty も同様
}
```

### FFI バックエンドの共通化: Ops テーブルパターン

macOS （CoreAudio）、PulseAudio、PipeWire の 3 つの FFI バックエンドは、いずれも C 実装を呼び出す構造が同一であるため、C 関数ポインタを格納した `DeviceOps` / `CaptureOps` / `PlaybackOps` という関数テーブルで共通化する。

#### 共通 FFI 型モジュール （`src/ffi_common.rs`）

`build.rs` で `audio.h` 単体に対して bindgen を実行し、全バックエンドで共有する型と定数を生成する。生成された `bindings_common.rs` を `src/ffi_common.rs` で include する。このモジュールは以下を宣言する:

```rust
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
```

`audio.h` の完全な内容は以下のとおり:

```c
#pragma once
#include <stdint.h>

#if defined(__cplusplus)
extern "C" {
#endif

struct AudioDevice;
struct AudioSession;

#define AUDIO_FORMAT_S16 0
#define AUDIO_FORMAT_F32 1

#define AUDIO_DEVICE_TYPE_INPUT  0
#define AUDIO_DEVICE_TYPE_OUTPUT 1

typedef void (*AudioFrameCallback)(void* user_data,
                                    const void* data,
                                    int frames,
                                    int channels,
                                    int sample_rate,
                                    int format,
                                    int64_t timestamp_us);

#if defined(__cplusplus)
}
#endif
```

各バックエンドヘッダ （`audio_ca.h` 等） は `#include "audio.h"` する。バックエンド別の bindgen では `audio.h` 由来の型が重複生成されるが、`#[allow]` で抑制する。

#### バックエンド別 bindgen と型共有の設計

各バックエンドヘッダ （`audio_ca.h` 等） に対して bindgen を実行し、それぞれ `bindings_ca.rs` / `bindings_pulse.rs` / `bindings_pipewire.rs` を生成する。各バックエンドヘッダが `#include "audio.h"` しているため、共通型もバックエンド別 bindgen 出力に含まれる。

`DeviceOps` / `CaptureOps` の関数ポインタは `ffi_common` モジュールの型を参照する。バックエンド別 bindgen の関数はローカルモジュールの型を参照するため、ops テーブル定義時にポインタキャストを行う:

```rust
// src/device_ffi.rs
#[cfg(enable_ca)]
const OPS_CA: DeviceOps = DeviceOps {
    device_name: ffi_ca::audio_ca_device_name as unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> *const c_char,
    device_unique_id: ffi_ca::audio_ca_device_unique_id as unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> *const c_char,
    device_channels: ffi_ca::audio_ca_device_channels as unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> i32,
    device_sample_rate: ffi_ca::audio_ca_device_sample_rate as unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> i32,
    device_type: ffi_ca::audio_ca_device_type as unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> i32,
    enumerate_devices: ffi_ca::audio_ca_enumerate_devices as unsafe extern "C" fn(*mut *mut *mut ffi_common::AudioDevice, *mut i32) -> i32,
    free_devices: ffi_ca::audio_ca_free_devices as unsafe extern "C" fn(*mut *mut ffi_common::AudioDevice, i32),
};
```

いずれの型も C 側では不透明ポインタ （`struct AudioDevice;` の前方宣言のみ） であり、全バックエンドでメモリ表現は同一のため、キャストは安全である。`#[cfg(enable_pulse)]` の `OPS_PULSE`、`#[cfg(enable_pipewire)]` の `OPS_PIPEWIRE` も同様に定義する。

#### DeviceOps

```rust
// src/device_ffi.rs
use crate::ffi_common;

struct DeviceOps {
    device_name: unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> *const c_char,
    device_unique_id: unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> *const c_char,
    device_channels: unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> i32,
    device_sample_rate: unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> i32,
    device_type: unsafe extern "C" fn(*mut ffi_common::AudioDevice) -> i32,
    enumerate_devices: unsafe extern "C" fn(*mut *mut *mut ffi_common::AudioDevice, *mut i32) -> i32,
    free_devices: unsafe extern "C" fn(*mut *mut ffi_common::AudioDevice, i32),
}

// DeviceOps は 'static な関数ポインタのみを含むため Sync が自動導出される。
// 明示的に unsafe impl を付与する必要はない。
```

#### CaptureOps

```rust
// src/capture_ffi.rs
use crate::ffi_common;

struct CaptureOps {
    session_create: unsafe extern "C" fn(*const c_char, i32, i32) -> *mut ffi_common::AudioSession,
    session_start: unsafe extern "C" fn(*mut ffi_common::AudioSession, ffi_common::AudioFrameCallback, *mut c_void) -> i32,
    session_stop: unsafe extern "C" fn(*mut ffi_common::AudioSession),
    session_destroy: unsafe extern "C" fn(*mut ffi_common::AudioSession),
    session_sample_rate: unsafe extern "C" fn(*mut ffi_common::AudioSession) -> i32,
    session_channels: unsafe extern "C" fn(*mut ffi_common::AudioSession) -> i32,
}
```

#### PlaybackOps

```rust
// src/playback_ffi.rs
// 再生は macOS/Linux で未実装のため、現時点では空だが将来の拡張用に用意する
struct PlaybackOps {}
```

### 具象型のフィールド定義

#### FfiDeviceImpl / FfiDeviceListImpl

```rust
// src/device_ffi.rs
pub(crate) struct FfiDeviceImpl {
    ops: &'static DeviceOps,
    ptr: NonNull<ffi_common::AudioDevice>,
    device_type: AudioDeviceType,
}

pub(crate) struct FfiDeviceListImpl {
    devices: Vec<AudioDevice>,  // AudioDevice newtype のリスト
    raw_ptr: *mut *mut ffi_common::AudioDevice,
    raw_count: i32,
    ops: &'static DeviceOps,
}
```

`FfiDeviceListImpl::enumerate()` は `ops.enumerate_devices` で生ポインタ配列を取得し、各要素から `FfiDeviceImpl` を構築して `AudioDevice` newtype で包み `Vec<AudioDevice>` に格納する。`raw_ptr` と `raw_count` は `Drop` での解放用に保持する。

#### FfiCaptureImpl

```rust
// src/capture_ffi.rs
pub(crate) struct FfiCaptureImpl {
    ops: &'static CaptureOps,
    session: Option<NonNull<ffi_common::AudioSession>>,
    context: Option<Arc<CaptureContext>>,
    config: AudioCaptureConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
}
```

`FfiCaptureImpl` と `FfiDeviceListImpl` の ops 注入は以下の呼出し連鎖で行う。バックエンド明示関数 （`AudioCapture::new_ca()` 等） が `&'static CaptureOps` 定数を `FfiCaptureImpl` のコンストラクタに注入し、具象型内部で ops テーブル経由の C 関数呼出しを行う:

```rust
// capture.rs の newtype 側 (例: new_pulse)
impl AudioCapture {
    #[cfg(enable_pulse)]
    pub fn new_pulse<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where F: Fn(AudioFrame<'_>) + Send + Sync + 'static
    {
        let inner = FfiCaptureImpl::new(&capture_ffi::CAPTURE_OPS_PULSE, config, callback)?;
        Ok(AudioCapture(AudioCaptureInner::Ffi(inner)))
    }
}

// capture_ffi.rs の具象型側
impl FfiCaptureImpl {
    pub(crate) fn new<F>(ops: &'static CaptureOps, config: AudioCaptureConfig, callback: F) -> Result<Self>
    where F: Fn(AudioFrame<'_>) + Send + Sync + 'static
    {
        let session = unsafe { (ops.session_create)(device_id_ptr, config.sample_rate, config.channels) };
        let session = NonNull::new(session).ok_or(Error::SessionCreateFailed)?;
        let actual_sample_rate = unsafe { (ops.session_sample_rate)(session.as_ptr()) };
        let actual_channels = unsafe { (ops.session_channels)(session.as_ptr()) };
        let context = Arc::new(CaptureContext { callback: Box::new(callback), running: AtomicBool::new(false) });
        Ok(Self { ops, session: Some(session), context: Some(context), config, actual_sample_rate, actual_channels })
    }
}
```

`FfiDeviceListImpl` も同様に `pub(crate) fn enumerate(ops: &'static DeviceOps, filter: Option<AudioDeviceType>) -> Result<Self>` のシグネチャを持ち、`ops.enumerate_devices` でデバイス取得後、`filter` が `Some` の場合は `device_type` でフィルタする。`filter` が `None` の場合は全デバイスを対象とする。これにより `enumerate()` / `enumerate_input()` / `enumerate_output()` の 3 つの公開メソッドがいずれも `FfiDeviceListImpl::enumerate(ops, filter)` に委譲される。

`FfiCaptureImpl` の `start()` / `stop()` は `ops.session_start` / `ops.session_stop` を呼ぶ。`Drop` では `stop()` 後に `ops.session_destroy` を呼ぶ。

`FfiDeviceImpl` の各アクセサ （`name()`, `unique_id()` 等） は `self.ops` の対応する関数ポインタを `self.ptr.as_ptr()` で呼ぶ。`FfiDeviceListImpl` の `Drop` は `self.ops.free_devices(self.raw_ptr, self.raw_count)` を呼び、`Vec<AudioDevice>` の drop は自動で行われる。C 側の `free_devices` が各デバイスの `free()` を行い、Rust 側の `FfiDeviceImpl` が持つ `NonNull` は `Drop` 実装を持たないため二重解放は発生しない。

#### FfiPlaybackImpl

```rust
// src/playback_ffi.rs
pub(crate) struct FfiPlaybackImpl {
    ops: &'static PlaybackOps,
    config: AudioPlaybackConfig,
}
```

`FfiPlaybackImpl::new()` は `Err(Error::SessionCreateFailed)` を返すスタブ。`start()` は `Err(Error::SessionStartFailed)` を返す。`PlaybackOps {}` は将来の拡張用のプレースホルダ。

#### WasapiDeviceImpl / WasapiDeviceListImpl

`device_wasapi.rs` は既存 `device_windows.rs` の `AudioDevice` / `AudioDeviceList` をそれぞれ `WasapiDeviceImpl` / `WasapiDeviceListImpl` にリネームしたもの。`WasapiDeviceListImpl.devices` は `Vec<AudioDevice>` （newtype） を保持する。`enumerate_devices_by_type` 内では `WasapiDeviceImpl` を構築し `AudioDevice(AudioDeviceInner::Wasapi(...))` で包む。`AudioDeviceInner` とそのバリアントは `pub(crate)` とすることで、`device_wasapi.rs` からアクセス可能にする。

`device_ffi.rs` と `device_wasapi.rs` はそれぞれ `device::AudioDevice` と `device::AudioDeviceInner` を import する。Rust のモジュールシステム上、`device.rs` が `device_ffi` / `device_wasapi` を `#[cfg]` 付きで子モジュール宣言し、子モジュールが親の `AudioDevice` / `AudioDeviceInner` を `use super::*` または明示的 import で参照する形で循環参照なく実現できる。

#### WasapiCaptureImpl / WasapiPlaybackImpl

`capture_wasapi.rs` / `playback_wasapi.rs` は既存の実装をそれぞれ `WasapiCaptureImpl` / `WasapiPlaybackImpl` にリネームして保持する。独自のコンテキスト型を内部に定義する。

### `Drop` 実装の設計

- `FfiDeviceListImpl` は `Drop` で `self.ops.free_devices(self.raw_ptr, self.raw_count)` を呼ぶ。`Vec<AudioDevice>` の drop は自動的に行われる
- `WasapiDeviceListImpl` は `Drop` 不要 （`Vec` が自動解放する）
- `FfiCaptureImpl` は `Drop` で `stop()` → `ops.session_destroy` を呼ぶ
- enum newtype （`AudioDeviceList`, `AudioCapture`, `AudioPlayback`） には `Drop` 実装を置かず、各具象型の `Drop` に委譲する

### `Send` / `Sync` 実装の引き継ぎ

現在すべての公開型に付与されている `unsafe impl Send` / `unsafe impl Sync` は、enum newtype 化後も引き続き必要:

- `AudioDevice`, `AudioDeviceList`, `AudioCapture`, `AudioPlayback` の各 newtype に `unsafe impl Send` / `unsafe impl Sync` を付与する
- 内部の具象型 （`FfiDeviceImpl`, `WasapiDeviceImpl` 等） にも `unsafe impl Send` / `unsafe impl Sync` を付与する
- `capture_wasapi.rs` 内の `unsafe impl Send for SendPtr<IAudioCaptureClient>` は、`SendPtr` が `device_wasapi.rs` に移動した後も `capture_wasapi.rs` 側で宣言する。`playback_wasapi.rs` の `unsafe impl Send for SendPtr<IAudioRenderClient>` と `unsafe impl Send for SendPtr<IAudioClient>` も同様に移行する

### FFI シンボル衝突の解決

#### C 構造体名の衝突回避

`audio_c.h` では `struct AudioDevice { ... };` が完全定義されており、複数バックエンドが同一バイナリにリンクされると One Definition Rule 違反となる。これを回避するため、各バックエンドで異なる構造体名を使用する:

| 現在 | 変更後 |
|---|---|
| `struct AudioDevice` | `struct AudioDeviceCA` / `struct AudioDevicePulse` / `struct AudioDevicePipeWire` |
| `struct AudioSession` | `struct AudioSessionCA` / `struct AudioSessionPulse` / `struct AudioSessionPipeWire` |

`audio.h` の前方宣言 （`struct AudioDevice;` / `struct AudioSession;`） は変更不要。各バックエンドの `.c` / `.m` ファイルでのみ完全定義の構造体名を変更する。Rust 側の bindgen は前方宣言により不透明型を生成するため、C 側の完全定義の構造体名変更の影響を受けない。

#### C 関数名にバックエンド接頭辞を付加する

| 現在の関数名 | macOS | PulseAudio | PipeWire |
|---|---|---|---|
| `audio_enumerate_devices` | `audio_ca_enumerate_devices` | `audio_pulse_enumerate_devices` | `audio_pipewire_enumerate_devices` |
| `audio_free_devices` | `audio_ca_free_devices` | `audio_pulse_free_devices` | `audio_pipewire_free_devices` |
| `audio_device_name` | `audio_ca_device_name` | `audio_pulse_device_name` | `audio_pipewire_device_name` |
| `audio_device_unique_id` | `audio_ca_device_unique_id` | `audio_pulse_device_unique_id` | `audio_pipewire_device_unique_id` |
| `audio_device_channels` | `audio_ca_device_channels` | `audio_pulse_device_channels` | `audio_pipewire_device_channels` |
| `audio_device_sample_rate` | `audio_ca_device_sample_rate` | `audio_pulse_device_sample_rate` | `audio_pipewire_device_sample_rate` |
| `audio_device_type` | `audio_ca_device_type` | `audio_pulse_device_type` | `audio_pipewire_device_type` |
| `audio_session_create` | `audio_ca_session_create` | `audio_pulse_session_create` | `audio_pipewire_session_create` |
| `audio_session_destroy` | `audio_ca_session_destroy` | `audio_pulse_session_destroy` | `audio_pipewire_session_destroy` |
| `audio_session_start` | `audio_ca_session_start` | `audio_pulse_session_start` | `audio_pipewire_session_start` |
| `audio_session_stop` | `audio_ca_session_stop` | `audio_pulse_session_stop` | `audio_pipewire_session_stop` |
| `audio_session_sample_rate` | `audio_ca_session_sample_rate` | `audio_pulse_session_sample_rate` | `audio_pipewire_session_sample_rate` |
| `audio_session_channels` | `audio_ca_session_channels` | `audio_pulse_session_channels` | `audio_pipewire_session_channels` |

#### ヘッダ分割

- `audio_c.h` を廃止し `audio.h` （共通型/定数） + `audio_ca.h` + `audio_pulse.h` + `audio_pipewire.h` に分割
- 各バックエンドヘッダ （`audio_ca.h` 等） は `#include "audio.h"` し、自バックエンドの `audio_<backend>_*` 関数プロトタイプのみを宣言する

#### C 実装ファイルの変更

| 現在 | 変更後 |
|---|---|
| `src/audio_c.m` | `src/audio_ca.m` にリネーム、関数名に `audio_ca_` 接頭辞付加、`struct AudioDevice` → `struct AudioDeviceCA`、`#include "audio_ca.h"` に変更 |
| `src/audio_pulse.c` | 関数名に `audio_pulse_` 接頭辞付加、`struct AudioDevice` → `struct AudioDevicePulse`、`#include "audio_pulse.h"` に変更 |
| `src/audio_pipewire.c` | 関数名に `audio_pipewire_` 接頭辞付加、`struct AudioDevice` → `struct AudioDevicePipeWire`、`#include "audio_pipewire.h"` に変更 |

### build.rs の変更

#### bindgen の多重実行

`build.rs` は以下の 4 回の bindgen 実行を行う。各バックエンドが無効な場合は該当する bindgen をスキップする:

1. **`audio.h`** → `bindings_common.rs`: `.allowlist_type("AudioDevice").allowlist_type("AudioSession").allowlist_type("AudioFrameCallback").allowlist_var("AUDIO_FORMAT_.*").allowlist_var("AUDIO_DEVICE_TYPE_.*")`

2. **`audio_ca.h`** （`enable_ca` 時のみ） → `bindings_ca.rs`: `.allowlist_function("audio_ca_.*")`。ヘッダが `#include "audio.h"` しているため共通型も生成されるが `#[allow]` で重複警告を抑制する。

3. **`audio_pulse.h`** （`enable_pulse` 時のみ） → `bindings_pulse.rs`: `.allowlist_function("audio_pulse_.*")`

4. **`audio_pipewire.h`** （`enable_pipewire` 時のみ） → `bindings_pipewire.rs`: `.allowlist_function("audio_pipewire_.*")`

各 bindgen には `.derive_default(true).derive_debug(true).parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))` を共通で指定する。

#### C コンパイルの選択

- `wasapi` feature 有効かつ非 Windows → `cargo:warning` を出力し、`enable_wasapi` cfg は生成しない
- `ca` feature 有効かつ非 macOS → `cargo:warning` を出力し、`enable_ca` cfg は生成しない
- `pulse` feature 有効かつ非 Linux → `cargo:warning` を出力し、`enable_pulse` cfg は生成しない
- `pipewire` feature 有効かつ非 Linux → `cargo:warning` を出力し、`enable_pipewire` cfg は生成しない
- `wasapi` feature 無効かつ Windows → `compile_error!` （`lib.rs` で）

相互排他 panic は削除し、`pulse` と `pipewire` の両方が有効かつ Linux の場合は両方コンパイルする。`pkg-config` も両方 probe する。

`default` feature は全プラットフォームのデフォルトバックエンドを含むが、プラットフォーム非適合の feature に対しては C コンパイルをスキップし cfg も生成しない。結果として、Linux で `cargo build` すると `windows` crate がダウンロード・コンパイルされるオーバーヘッドが発生する。このオーバーヘッドは許容する。

#### cfg フラグ生成

build.rs は以下の cfg を `cargo:rustc-cfg` で生成する:

| cfg 名 | 生成条件 |
|---|---|
| `enable_ca` | `ca` feature 有効かつ target_os = "macos" |
| `enable_pulse` | `pulse` feature 有効かつ target_os = "linux" |
| `enable_pipewire` | `pipewire` feature 有効かつ target_os = "linux" |
| `enable_wasapi` | `wasapi` feature 有効かつ target_os = "windows" |
| `enable_default_ca` | `enable_ca` が生成された場合 |
| `enable_default_pulse` | `enable_pulse` が生成された場合 （`pipewire` との両方が有効な場合も pulse がデフォルト） |
| `enable_default_pipewire` | `enable_pipewire` が生成され、かつ `enable_pulse` が生成されなかった場合 |
| `enable_default_wasapi` | `enable_wasapi` が生成された場合 |

`enable_default_*` は `AudioDeviceList::enumerate()` 等のデフォルトバックエンド選択に用いる。

#### rerun-if-changed

`audio.h`, `audio_ca.h`, `audio_pulse.h`, `audio_pipewire.h`, `audio_ca.m`, `audio_pulse.c`, `audio_pipewire.c` の全ファイルに対して `cargo::rerun-if-changed` を設定する。既存の `cargo::rerun-if-changed=src/audio_c.h` は削除する。

### feature flag の変更

```toml
[features]
default = ["default-pulse", "default-pipewire", "default-ca", "default-wasapi"]
ca = []
pulse = []
pipewire = []
wasapi = ["dep:windows"]
default-ca = ["ca"]
default-pulse = ["pulse"]
default-pipewire = ["pipewire"]
default-wasapi = ["wasapi"]
```

また `windows` 依存を `[target.'cfg(windows)'.dependencies]` から `[dependencies]` に移動し `optional = true` を付与する:

```toml
[dependencies]
windows = { version = "0.62", optional = true, features = [
  # ... （既存の features を維持）
] }
```

`dep:windows` により `wasapi` feature が有効な場合のみ `windows` crate がコンパイルされる。

### `src/lib.rs` の再エクスポート設計

```rust
#[cfg(not(any(enable_ca, enable_pulse, enable_pipewire, enable_wasapi)))]
compile_error!("No audio backend is enabled for this target. Enable at least one of: ca, pulse, pipewire, wasapi");

mod common;
mod error;
mod device;
mod capture;
mod playback;

#[cfg(any(enable_ca, enable_pulse, enable_pipewire))]
mod ffi_common;
#[cfg(enable_ca)]
mod ffi_ca;
#[cfg(enable_pulse)]
mod ffi_pulse;
#[cfg(enable_pipewire)]
mod ffi_pipewire;

#[cfg(any(enable_ca, enable_pulse, enable_pipewire))]
mod device_ffi;
#[cfg(enable_wasapi)]
mod device_wasapi;
#[cfg(any(enable_ca, enable_pulse, enable_pipewire))]
mod capture_ffi;
#[cfg(enable_wasapi)]
mod capture_wasapi;
#[cfg(any(enable_ca, enable_pulse, enable_pipewire))]
mod playback_ffi;
#[cfg(enable_wasapi)]
mod playback_wasapi;

pub use common::{
    AudioCaptureConfig, AudioDeviceType, AudioFormat, AudioFrame, AudioFrameOwned,
    AudioPlaybackConfig, PlaybackFrame,
};
pub use device::{AudioDevice, AudioDeviceList};
pub use capture::AudioCapture;
pub use playback::AudioPlayback;
pub use error::{Error, Result};
```

### デフォルトバックエンド選択ロジック

`AudioDeviceList::enumerate()` / `AudioCapture::new()` / `AudioPlayback::new()` のデフォルト実装は `#[cfg]` で以下の優先順位に従う。どのアームも有効でない場合は `lib.rs` 先頭の `compile_error!` が先に発動するため、ここでの fallback は不要:

```rust
impl AudioDeviceList {
    #[cfg(enable_default_ca)]
    pub fn enumerate() -> Result<Self> { /* ca で列挙 */ }
    #[cfg(all(enable_default_pulse, not(enable_default_ca)))]
    pub fn enumerate() -> Result<Self> { /* pulse で列挙 */ }
    #[cfg(all(enable_default_pipewire, not(any(enable_default_ca, enable_default_pulse))))]
    pub fn enumerate() -> Result<Self> { /* pipewire で列挙 */ }
    #[cfg(all(enable_default_wasapi, not(any(enable_default_ca, enable_default_pulse, enable_default_pipewire))))]
    pub fn enumerate() -> Result<Self> { /* wasapi で列挙 */ }
}
```

### バックエンド明示用の関連関数

```rust
impl AudioDeviceList {
    #[cfg(enable_ca)]
    pub fn enumerate_ca() -> Result<Self> { /* macOS CoreAudio */ }
    #[cfg(enable_pulse)]
    pub fn enumerate_pulse() -> Result<Self> { /* Linux PulseAudio */ }
    #[cfg(enable_pipewire)]
    pub fn enumerate_pipewire() -> Result<Self> { /* Linux PipeWire */ }
    #[cfg(enable_wasapi)]
    pub fn enumerate_wasapi() -> Result<Self> { /* Windows WASAPI */ }
}

impl AudioCapture {
    #[cfg(enable_ca)]
    pub fn new_ca<F>(config: AudioCaptureConfig, callback: F) -> Result<Self> { /* macOS */ }
    #[cfg(enable_pulse)]
    pub fn new_pulse<F>(config: AudioCaptureConfig, callback: F) -> Result<Self> { /* PulseAudio */ }
    #[cfg(enable_pipewire)]
    pub fn new_pipewire<F>(config: AudioCaptureConfig, callback: F) -> Result<Self> { /* PipeWire */ }
    #[cfg(enable_wasapi)]
    pub fn new_wasapi<F>(config: AudioCaptureConfig, callback: F) -> Result<Self> { /* WASAPI */ }
}

impl AudioPlayback {
    #[cfg(enable_ca)]
    pub fn new_ca<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self> { /* macOS */ }
    #[cfg(enable_pulse)]
    pub fn new_pulse<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self> { /* PulseAudio */ }
    #[cfg(enable_pipewire)]
    pub fn new_pipewire<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self> { /* PipeWire */ }
    #[cfg(enable_wasapi)]
    pub fn new_wasapi<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self> { /* WASAPI */ }
}
```

### AudioDeviceType / AudioFormat の from_ffi の配置

`AudioDeviceType::from_ffi()` と `AudioFormat::from_ffi()` は `src/device_ffi.rs` に移動する。移動先では `ffi_common` モジュールの定数 （`ffi_common::AUDIO_DEVICE_TYPE_*` 等） を参照する。全 FFI バックエンドでこれらの定数値は同一であるため、`ffi_common` からの参照で全バックエンドに対応できる。

可視性は `pub(crate)` とする。現在 `AudioFormat::from_ffi()` は `pub(crate)` （`device.rs:19`）、`AudioDeviceType::from_ffi()` は `fn` （private） であり、引き継ぐ。

### CaptureContext の移動

現在 `src/common.rs` に定義されている `CaptureContext` は `src/capture_ffi.rs` に移動し、`common.rs` からは削除する。`Sync` 境界は維持する。これに伴い `src/common.rs` の `use std::sync::atomic::AtomicBool;` import も削除する。

`capture_wasapi.rs` は独自のコンテキスト型 `WasapiCaptureContext` を内部に定義する。フィールドは `CaptureContext` と同等 （`callback`, `running`） とする。`PlaybackContext` （現在 `playback_windows.rs`） は `playback_wasapi.rs` に残す。

### ファイル構成

| ファイル | 役割 |
|---|---|
| `src/lib.rs` | クレートルート。モジュール宣言と re-export。先頭に `compile_error!` ガードを追加 |
| `src/common.rs` | 共通型 （`CaptureContext` を除く現行の全型を維持） |
| `src/error.rs` | `Error` 列挙型。`ComInitFailed` バリアントは常に存在させるが、`wasapi` 非有効時は構築されない |
| `src/device.rs` （上書き） | 公開型 `AudioDevice` (newtype), `AudioDeviceList` (newtype)。内部 enum と委譲実装 |
| `src/device_ffi.rs` （新規） | FFI バックエンド共通: `FfiDeviceImpl`, `FfiDeviceListImpl`, `DeviceOps`。`AudioDeviceType::from_ffi()`, `AudioFormat::from_ffi()` をここに移動 |
| `src/device_wasapi.rs` （新規） | WASAPI 実装: `WasapiDeviceImpl`, `WasapiDeviceListImpl`。`SendHandle`, `SendPtr`, `init_com_mta()`, `determine_audio_format()`, `get_device_by_id()` は `pub(crate)` で引き続き提供 |
| `src/capture.rs` （上書き） | 公開型 `AudioCapture` (newtype)。内部 enum と委譲実装 |
| `src/capture_ffi.rs` （新規） | FFI バックエンド共通: `FfiCaptureImpl`, `CaptureOps`, `CaptureContext` （移動）, `frame_callback` |
| `src/capture_wasapi.rs` （新規） | WASAPI キャプチャ実装: `WasapiCaptureImpl`, `SessionData` （独自コンテキスト型を含む） |
| `src/playback.rs` （上書き） | 公開型 `AudioPlayback` (newtype)。内部 enum と委譲実装 |
| `src/playback_ffi.rs` （新規） | FFI バックエンド共通: `FfiPlaybackImpl`, `PlaybackOps` （現時点では未実装スタブ） |
| `src/playback_wasapi.rs` （新規） | WASAPI 再生実装: `WasapiPlaybackImpl`, `SessionData`, `PlaybackContext` |
| `src/ffi_common.rs` （新規） | `audio.h` の bindgen include。共通型と定数。`#[allow(...)]` を宣言 |
| `src/ffi_ca.rs` （新規） | CoreAudio bindgen include。`#[allow(...)]` を宣言 |
| `src/ffi_pulse.rs` （新規） | PulseAudio bindgen include。`#[allow(...)]` を宣言 |
| `src/ffi_pipewire.rs` （新規） | PipeWire bindgen include。`#[allow(...)]` を宣言 |
| `build.rs` | プラットフォーム別コンパイル、バックエンド別 bindgen、cfg 生成 |
| `src/audio.h` （新規） | 共通 C ヘッダ （型定義、定数。完全内容を上記に記載） |
| `src/audio_ca.h` （新規） | CoreAudio ヘッダ |
| `src/audio_pulse.h` （新規） | PulseAudio ヘッダ |
| `src/audio_pipewire.h` （新規） | PipeWire ヘッダ |
| `src/audio_ca.m` （新規, audio_c.m からリネーム） | CoreAudio 実装、関数名に `audio_ca_` 接頭辞付加 |
| `src/audio_pulse.c` | PulseAudio 実装、関数名に `audio_pulse_` 接頭辞付加 |
| `src/audio_pipewire.c` | PipeWire 実装、関数名に `audio_pipewire_` 接頭辞付加 |

### 削除されるファイル

| ファイル | 理由 |
|---|---|
| `src/device.rs` （旧） | 上書き。`from_ffi` は `device_ffi.rs` に移行 |
| `src/device_windows.rs` | `device_wasapi.rs` に移行 |
| `src/capture.rs` （旧） | 上書き |
| `src/capture_windows.rs` | `capture_wasapi.rs` に移行 |
| `src/playback.rs` （旧） | 上書き |
| `src/playback_windows.rs` | `playback_wasapi.rs` に移行 |
| `src/ffi.rs` | `ffi_common.rs` + `ffi_ca.rs` + `ffi_pulse.rs` + `ffi_pipewire.rs` に分割 |
| `src/audio_c.h` | `audio.h` + `audio_ca.h` + `audio_pulse.h` + `audio_pipewire.h` に分割 |
| `src/audio_c.m` | `audio_ca.m` にリネーム |

### コールバックの配置

`extern "C" fn frame_callback` は `capture_ffi.rs` に単一で配置する。`AudioFrameCallback` のシグネチャは全 FFI バックエンドで同一 （`ffi_common::AudioFrameCallback`） であるため、1 つのコールバック関数で全 FFI バックエンドを処理できる。Windows WASAPI にはコールバックは不要 （専用スレッドで駆動するため）。

### `src/error.rs` の変更

`Error::ComInitFailed` の cfg 属性を enum 定義と `Display` 実装の両方から削除し、常に存在するバリアントにする。`wasapi` feature が無効な場合、このバリアントが実行時に構築されることはないが、外部クレートのパターンマッチではワイルドカードで扱う必要がある。

```rust
// error.rs 変更後: enum 定義から #[cfg] を削除
pub enum Error {
    DeviceNotFound,
    DeviceAccessDenied,
    SessionCreateFailed,
    SessionStartFailed,
    ComInitFailed,  // cfg 属性を削除。wasapi 非有効時は構築されないがバリアントとして常に存在
    NullPointer(&'static str),
    InvalidChannels,
    DataTooLarge(std::num::TryFromIntError),
    UnknownFormat(i32),
    UnknownDeviceType(i32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // ...
            Error::ComInitFailed => write!(f, "COM initialization failed: ..."),  // #[cfg] を削除
            // ...
        }
    }
}
```

### コールバック境界

callback 境界は既存の `Fn + Send + Sync + 'static` を維持する:

- `AudioCapture::new`: `F: Fn(AudioFrame<'_>) + Send + Sync + 'static`
- `AudioPlayback::new`: `F: Fn() -> Option<PlaybackFrame> + Send + Sync + 'static`
- `CaptureContext.callback`: `Box<dyn Fn(AudioFrame<'_>) + Send + Sync>`

## 破壊的変更

- `AudioDevice`, `AudioDeviceList`, `AudioCapture`, `AudioPlayback` の内部実装が enum に変更される （API は維持）
- `Cargo.toml` の feature flag が変更される （`pulse` / `pipewire` → 新体系、`wasapi` 追加）
- `Error::ComInitFailed` の cfg 属性が削除され、常に存在するバリアントになる
- `windows` crate が常時依存から `wasapi` feature 経由のオプショナル依存に変わる
- 利用者がバックエンド別の関数を明示的に呼ぶ場合、新しい命名規則に従う必要がある

## テスト戦略

### 単体テスト

- `src/device.rs` 内 `#[cfg(test)] mod tests`:
  - `AudioDeviceType::from_ffi()` の既存テストを `src/device_ffi.rs` の `#[cfg(test)] mod tests` に移行
  - `AudioFormat::from_ffi()` の既存テストを `src/device_ffi.rs` の `#[cfg(test)] mod tests` に移行
  - 移行後のテストは `ffi_common` モジュールの定数を参照するよう更新
- `src/device_ffi.rs` 内 `#[cfg(test)] mod tests`:
  - `DeviceOps` の各フィールドが期待する関数ポインタを持つことのテスト
  - FFI バックエンド別の Ops テーブルが正しく構築されることのテスト
- `src/device_wasapi.rs` 内 `#[cfg(test)] mod tests`:
  - 既存の `from_ffi` 相当のテストは不要 （WASAPI は FFI 定数を使わない）
- `tests/test_common.rs`:
  - 変更不要
- `pbt/tests/prop_error.rs`:
  - `ComInitFailed` が常に存在するため、`#[cfg(target_os = "windows")]` の cfg 分岐が不要になる。`ComInitFailed` バリアントを常に `arb_error()` の候補に含める

### PBT

- `pbt/tests/prop_capture.rs`, `pbt/tests/prop_playback.rs`: 変更不要 （crate root からの re-export を import しているため）
- `pbt/tests/prop_error.rs`: `ComInitFailed` を常に含めるよう変更

### Fuzzing

- `fuzz/` 以下の fuzzing ターゲットは、`target_os = "windows"` を条件とする cfg がある場合は確認し、必要に応じて `enable_wasapi` 対応に変更する
- モジュール import path の変更がないか確認する

### カバレッジ

変更後のカバレッジ取得コマンド:

```bash
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report -p shiguredo_audio_device --lib -- device_ffi
cargo llvm-cov --no-report -p shiguredo_audio_device --test test_common
cargo llvm-cov --no-report -p shiguredo_audio_device --test prop_capture
cargo llvm-cov --no-report -p shiguredo_audio_device --test prop_playback
cargo llvm-cov --no-report -p shiguredo_audio_device --test prop_error
cargo llvm-cov report
```

## CHANGES.md への追記

`## develop` セクションの既存 `[FIX]` エントリ群よりも前に以下のエントリを追記する （`CHANGES.md` の種別順序規定: UPDATE → ADD → CHANGE → FIX）:

```
- [CHANGE] AudioDevice, AudioDeviceList, AudioCapture, AudioPlayback を enum newtype 化し pulse と pipewire を共存可能にする
  - @melpon
```

## 完了条件

1. `pulse` と `pipewire` の両 feature を同時に有効にして Linux 上でビルドが成功すること
2. `AudioDeviceList::enumerate()` がデフォルトバックエンドで正常に動作すること
3. `AudioDeviceList::enumerate_pulse()` / `AudioDeviceList::enumerate_pipewire()` でバックエンドを明示的に選択できること
4. 全既存 PBT / 単体テストが新しいファイル構成下でパスすること
5. 相互排他 `panic!` が `build.rs` から削除されていること
6. WASAPI （Windows） ビルドが `wasapi` feature 下で成功すること
7. CoreAudio （macOS） ビルドが `ca` feature 下で成功すること
8. 全バックエンド非有効時に `compile_error!` が意図したメッセージを出力すること

## 解決方法

（実装後に追記）
