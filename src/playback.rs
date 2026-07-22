//! オーディオ再生の定義。
//!
//! バックエンドに依存しない形で再生の開始・停止・設定取得を提供する
//! [AudioPlayback] を提供する。

use crate::common::{AudioPlaybackConfig, PlaybackFrame};
use crate::error::Result;

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
use crate::playback_ffi::FfiPlaybackImpl;

#[cfg(enable_wasapi)]
use crate::playback_wasapi::WasapiPlaybackImpl;

/// オーディオ再生。
pub struct AudioPlayback(AudioPlaybackInner);

pub(crate) enum AudioPlaybackInner {
    #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
    Ffi(FfiPlaybackImpl),
    #[cfg(enable_wasapi)]
    Wasapi(WasapiPlaybackImpl),
}

impl AudioPlayback {
    /// デフォルトバックエンドで再生を構築する。
    ///
    /// `callback` は音声データが必要になるたびに呼ばれる。
    /// 引数は `(frames, channels, sample_rate)` で、それぞれ要求フレーム数・
    /// チャンネル数・サンプルレートを表す。
    /// `None` を返すと無音が再生される。
    #[cfg(any(
        enable_default_coreaudio,
        enable_default_pulse,
        enable_default_pipewire,
        enable_default_wasapi
    ))]
    pub fn new<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
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

    /// CoreAudio で再生を構築する。
    ///
    /// `callback` の引数は `(frames, channels, sample_rate)`。
    #[cfg(enable_coreaudio)]
    pub fn new_coreaudio<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Ok(Self(AudioPlaybackInner::Ffi(
            FfiPlaybackImpl::new_coreaudio(config, callback)?,
        )))
    }

    /// PulseAudio で再生を構築する。
    ///
    /// `callback` の引数は `(frames, channels, sample_rate)`。
    #[cfg(enable_pulse)]
    pub fn new_pulse<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Ok(Self(AudioPlaybackInner::Ffi(FfiPlaybackImpl::new_pulse(
            config, callback,
        )?)))
    }

    /// PipeWire で再生を構築する。
    ///
    /// `callback` の引数は `(frames, channels, sample_rate)`。
    #[cfg(enable_pipewire)]
    pub fn new_pipewire<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Ok(Self(AudioPlaybackInner::Ffi(
            FfiPlaybackImpl::new_pipewire(config, callback)?,
        )))
    }

    /// WASAPI で再生を構築する。
    ///
    /// `callback` の引数は `(frames, channels, sample_rate)`。
    #[cfg(enable_wasapi)]
    pub fn new_wasapi<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Ok(Self(AudioPlaybackInner::Wasapi(WasapiPlaybackImpl::new(
            config, callback,
        )?)))
    }

    /// 再生を開始する。
    pub fn start(&mut self) -> Result<()> {
        match &mut self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioPlaybackInner::Ffi(inner) => inner.start(),
            #[cfg(enable_wasapi)]
            AudioPlaybackInner::Wasapi(inner) => inner.start(),
        }
    }

    /// 再生を停止する。
    pub fn stop(&mut self) {
        match &mut self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioPlaybackInner::Ffi(inner) => inner.stop(),
            #[cfg(enable_wasapi)]
            AudioPlaybackInner::Wasapi(inner) => inner.stop(),
        }
    }

    /// 再生設定を取得する。
    pub fn config(&self) -> &AudioPlaybackConfig {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioPlaybackInner::Ffi(inner) => inner.config(),
            #[cfg(enable_wasapi)]
            AudioPlaybackInner::Wasapi(inner) => inner.config(),
        }
    }

    /// 実際のサンプルレートを取得する。
    pub fn sample_rate(&self) -> i32 {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioPlaybackInner::Ffi(inner) => inner.sample_rate(),
            #[cfg(enable_wasapi)]
            AudioPlaybackInner::Wasapi(inner) => inner.sample_rate(),
        }
    }

    /// 実際のチャンネル数を取得する。
    pub fn channels(&self) -> i32 {
        match &self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioPlaybackInner::Ffi(inner) => inner.channels(),
            #[cfg(enable_wasapi)]
            AudioPlaybackInner::Wasapi(inner) => inner.channels(),
        }
    }
}

unsafe impl Send for AudioPlayback {}
