//! オーディオキャプチャの定義。
//!
//! バックエンドに依存しない形でキャプチャの開始・停止・設定取得を提供する
//! [AudioCapture] を提供する。

use crate::common::{AudioCaptureConfig, AudioFrame};
use crate::error::Result;

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
use crate::capture_ffi::FfiCaptureImpl;

#[cfg(enable_wasapi)]
use crate::capture_wasapi::WasapiCaptureImpl;

/// オーディオキャプチャ。
pub struct AudioCapture(AudioCaptureInner);

pub(crate) enum AudioCaptureInner {
    #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
    Ffi(FfiCaptureImpl),
    #[cfg(enable_wasapi)]
    Wasapi(WasapiCaptureImpl),
}

impl AudioCapture {
    /// デフォルトバックエンドでキャプチャを構築する。
    #[cfg(any(
        enable_default_coreaudio,
        enable_default_pulse,
        enable_default_pipewire,
        enable_default_wasapi
    ))]
    pub fn new<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        #[cfg(enable_default_coreaudio)]
        {
            Self::new_coreaudio(config, callback)
        }
        #[cfg(enable_default_pulse)]
        {
            Self::new_pulse(config, callback)
        }
        #[cfg(enable_default_pipewire)]
        {
            Self::new_pipewire(config, callback)
        }
        #[cfg(enable_default_wasapi)]
        {
            Self::new_wasapi(config, callback)
        }
    }

    // -----------------------------------------------------------------------
    // CoreAudio 明示関数
    // -----------------------------------------------------------------------

    #[cfg(enable_coreaudio)]
    pub fn new_coreaudio<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        Ok(Self(AudioCaptureInner::Ffi(FfiCaptureImpl::new_coreaudio(
            config, callback,
        )?)))
    }

    // -----------------------------------------------------------------------
    // PulseAudio 明示関数
    // -----------------------------------------------------------------------

    #[cfg(enable_pulse)]
    pub fn new_pulse<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        Ok(Self(AudioCaptureInner::Ffi(FfiCaptureImpl::new_pulse(
            config, callback,
        )?)))
    }

    // -----------------------------------------------------------------------
    // PipeWire 明示関数
    // -----------------------------------------------------------------------

    #[cfg(enable_pipewire)]
    pub fn new_pipewire<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        Ok(Self(AudioCaptureInner::Ffi(FfiCaptureImpl::new_pipewire(
            config, callback,
        )?)))
    }

    // -----------------------------------------------------------------------
    // WASAPI 明示関数
    // -----------------------------------------------------------------------

    #[cfg(enable_wasapi)]
    pub fn new_wasapi<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        Ok(Self(AudioCaptureInner::Wasapi(WasapiCaptureImpl::new(
            config, callback,
        )?)))
    }

    // -----------------------------------------------------------------------
    // 共通 API
    // -----------------------------------------------------------------------

    /// キャプチャを開始する。
    pub fn start(&mut self) -> Result<()> {
        match &mut self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioCaptureInner::Ffi(inner) => inner.start(),
            #[cfg(enable_wasapi)]
            AudioCaptureInner::Wasapi(inner) => inner.start(),
        }
    }

    /// キャプチャを停止する。
    pub fn stop(&mut self) {
        match &mut self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioCaptureInner::Ffi(inner) => inner.stop(),
            #[cfg(enable_wasapi)]
            AudioCaptureInner::Wasapi(inner) => inner.stop(),
        }
    }

    /// キャプチャ設定を取得する。
    pub fn config(&self) -> &AudioCaptureConfig {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioCaptureInner::Ffi(inner) => inner.config(),
            #[cfg(enable_wasapi)]
            AudioCaptureInner::Wasapi(inner) => inner.config(),
        }
    }

    /// 実際のサンプルレートを取得する。
    pub fn sample_rate(&self) -> i32 {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioCaptureInner::Ffi(inner) => inner.sample_rate(),
            #[cfg(enable_wasapi)]
            AudioCaptureInner::Wasapi(inner) => inner.sample_rate(),
        }
    }

    /// 実際のチャンネル数を取得する。
    pub fn channels(&self) -> i32 {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioCaptureInner::Ffi(inner) => inner.channels(),
            #[cfg(enable_wasapi)]
            AudioCaptureInner::Wasapi(inner) => inner.channels(),
        }
    }
}

unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}
