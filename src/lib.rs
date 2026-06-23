//! shiguredo_audio_device - macOS/Linux/Windows 対応のオーディオライブラリ
//!
//! このクレートは macOS、Linux (PulseAudio/PipeWire)、Windows (WASAPI) をサポートしています。
//! 音声キャプチャ（マイク入力）と音声再生（スピーカー出力）の機能を提供します。

mod common;
mod error;

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
mod capture;
#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
mod device;
#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
mod playback;

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
mod capture_ffi;
#[cfg(enable_wasapi)]
mod capture_wasapi;
#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
mod device_ffi;
#[cfg(enable_wasapi)]
mod device_wasapi;
#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
mod ffi;
#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
mod playback_ffi;
#[cfg(enable_wasapi)]
mod playback_wasapi;

pub use common::{
    AudioCaptureConfig, AudioDeviceType, AudioFormat, AudioFrame, AudioFrameOwned,
    AudioPlaybackConfig, PlaybackFrame,
};

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
pub use capture::AudioCapture;
#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
pub use device::{AudioDevice, AudioDeviceList};
#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
pub use playback::AudioPlayback;

pub use error::{Error, Result};
