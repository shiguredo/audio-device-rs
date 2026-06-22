//! shiguredo_audio_device - macOS/Linux/Windows 対応のオーディオライブラリ
//!
//! このクレートは macOS、Linux (PipeWire)、Windows (WASAPI) をサポートしています。
//! 音声キャプチャ（マイク入力）と音声再生（スピーカー出力）の機能を提供します。

mod common;
mod error;

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod capture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod device;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod ffi;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod playback;

#[cfg(target_os = "windows")]
mod capture_windows;
#[cfg(target_os = "windows")]
mod device_windows;
#[cfg(target_os = "windows")]
mod playback_windows;

pub use common::{
    AudioCaptureConfig, AudioDeviceType, AudioFormat, AudioFrame, AudioFrameOwned,
    AudioPlaybackConfig, PlaybackFrame,
};

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use capture::AudioCapture;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use device::{AudioDevice, AudioDeviceList};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use playback::AudioPlayback;

#[cfg(target_os = "windows")]
pub use capture_windows::AudioCapture;
#[cfg(target_os = "windows")]
pub use device_windows::{AudioDevice, AudioDeviceList};
#[cfg(target_os = "windows")]
pub use playback_windows::AudioPlayback;

pub use error::{Error, Result};
