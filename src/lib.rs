//! shiguredo_audio_device - macOS/Linux/Windows 対応のオーディオライブラリ
//!
//! このクレートは macOS、Linux (PipeWire)、Windows (WASAPI) をサポートしています。
//! 音声キャプチャ（マイク入力）と音声再生（スピーカー出力）の機能を提供します。

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

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use capture::{AudioCapture, AudioCaptureConfig, AudioFrame, AudioFrameOwned};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use device::{AudioDevice, AudioDeviceList, AudioDeviceType, AudioFormat};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use playback::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

#[cfg(target_os = "windows")]
pub use capture_windows::{AudioCapture, AudioCaptureConfig, AudioFrame, AudioFrameOwned};
#[cfg(target_os = "windows")]
pub use device_windows::{AudioDevice, AudioDeviceList, AudioDeviceType, AudioFormat};
#[cfg(target_os = "windows")]
pub use playback_windows::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

pub use error::{Error, Result};
