//! オーディオキャプチャの定義。
//!
//! バックエンドに依存しない形でキャプチャの開始・停止・設定取得を提供する
//! [AudioCapture] と [AudioCaptureSession] を定義する。

use crate::common::{AudioCaptureConfig, AudioFrame};
use crate::error::Result;

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
use crate::capture_ffi::FfiCaptureImpl;

#[cfg(enable_wasapi)]
use crate::capture_wasapi::WasapiCaptureImpl;

/// オーディオキャプチャ。
///
/// キャプチャを開始するには [AudioCapture::start] を呼ぶ。
pub struct AudioCapture(AudioCaptureInner);

/// 実行中のオーディオキャプチャセッション。
///
/// [AudioCapture::start] が返す。この値が生存している間は [AudioCapture] を
/// 借用する。停止は [AudioCaptureSession::stop] で行い、`stop` を呼ばずに
/// drop した場合も停止する。Drop は実行中のユーザーコールバックの完了を待つ。
/// ガードは別のスレッドへ move できる。
#[must_use = "dropping the session stops the capture"]
pub struct AudioCaptureSession<'a> {
    /// セッションを開始したキャプチャ
    capture: &'a mut AudioCapture,
}

pub(crate) enum AudioCaptureInner {
    #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
    Ffi(FfiCaptureImpl),
    #[cfg(enable_wasapi)]
    Wasapi(WasapiCaptureImpl),
}

impl AudioCapture {
    /// デフォルトバックエンドでキャプチャを構築する。
    ///
    /// `config.sample_rate` と `config.channels` で指定したサンプルレートと
    /// チャンネル数の S16 ストリームでキャプチャする。
    /// `config.sample_rate` と `config.channels` が 0 以下の値は、それぞれ
    /// 48000 Hz、1 ch として扱う。
    ///
    /// `callback` には `Fn(AudioFrame<'_>) + Send + 'static` を要求する。
    /// `callback` はキャプチャ開始後、キャプチャしたフレームごとに、バックエンドが
    /// 管理するスレッド上で直列に呼ばれる。
    #[cfg(any(
        enable_default_coreaudio,
        enable_default_pulse,
        enable_default_pipewire,
        enable_default_wasapi
    ))]
    pub fn new<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + 'static,
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

    /// CoreAudio でキャプチャを構築する。
    ///
    /// `callback` はバックエンドが管理するスレッド上で直列に呼ばれる。
    #[cfg(enable_coreaudio)]
    pub fn new_coreaudio<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + 'static,
    {
        Ok(Self(AudioCaptureInner::Ffi(FfiCaptureImpl::new_coreaudio(
            config, callback,
        )?)))
    }

    // -----------------------------------------------------------------------
    // PulseAudio 明示関数
    // -----------------------------------------------------------------------

    /// PulseAudio でキャプチャを構築する。
    ///
    /// `callback` はバックエンドが管理するスレッド上で直列に呼ばれる。
    #[cfg(enable_pulse)]
    pub fn new_pulse<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + 'static,
    {
        Ok(Self(AudioCaptureInner::Ffi(FfiCaptureImpl::new_pulse(
            config, callback,
        )?)))
    }

    // -----------------------------------------------------------------------
    // PipeWire 明示関数
    // -----------------------------------------------------------------------

    /// PipeWire でキャプチャを構築する。
    ///
    /// `callback` はバックエンドが管理するスレッド上で直列に呼ばれる。
    #[cfg(enable_pipewire)]
    pub fn new_pipewire<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + 'static,
    {
        Ok(Self(AudioCaptureInner::Ffi(FfiCaptureImpl::new_pipewire(
            config, callback,
        )?)))
    }

    // -----------------------------------------------------------------------
    // WASAPI 明示関数
    // -----------------------------------------------------------------------

    /// WASAPI でキャプチャを構築する。
    ///
    /// `callback` はバックエンドが管理するスレッド上で直列に呼ばれる。
    #[cfg(enable_wasapi)]
    pub fn new_wasapi<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + 'static,
    {
        Ok(Self(AudioCaptureInner::Wasapi(WasapiCaptureImpl::new(
            config, callback,
        )?)))
    }

    // -----------------------------------------------------------------------
    // 共通 API
    // -----------------------------------------------------------------------

    /// キャプチャを開始する。
    ///
    /// 開始したセッションを表す [AudioCaptureSession] を返す。返されたガードが
    /// 生存している間はこの `AudioCapture` を借用するため、再度 `start` を
    /// 呼ぶことも、`AudioCapture` を drop することもできない。
    ///
    /// # Errors
    ///
    /// 開始に失敗した場合に `Error::SessionStartFailed` を返す。
    ///
    /// # セッション中の借用
    ///
    /// 次のコードは、セッションが `AudioCapture` を借用している間に再度
    /// `start` を呼ぶためコンパイルエラーになる。
    ///
    /// ```compile_fail,E0499
    /// fn cannot_start_twice(mut capture: shiguredo_audio_device::AudioCapture) {
    ///     let session = capture.start().expect("capture must start");
    ///     let _ = capture.start();
    ///     drop(session);
    /// }
    /// ```
    ///
    /// 次のコードは、セッションが `AudioCapture` を借用している間に
    /// `AudioCapture` を drop するためコンパイルエラーになる。
    ///
    /// ```compile_fail,E0505
    /// fn cannot_drop_while_running(mut capture: shiguredo_audio_device::AudioCapture) {
    ///     let session = capture.start().expect("capture must start");
    ///     drop(capture);
    ///     drop(session);
    /// }
    /// ```
    pub fn start(&mut self) -> Result<AudioCaptureSession<'_>> {
        match &mut self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioCaptureInner::Ffi(inner) => inner.start()?,
            #[cfg(enable_wasapi)]
            AudioCaptureInner::Wasapi(inner) => inner.start()?,
        }

        Ok(AudioCaptureSession { capture: self })
    }

    /// 実行中のセッションを停止する。
    fn stop_session(&mut self) {
        match &mut self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioCaptureInner::Ffi(inner) => inner.stop(),
            #[cfg(enable_wasapi)]
            AudioCaptureInner::Wasapi(inner) => inner.stop(),
        }
    }

    /// キャプチャ設定を取得する。
    ///
    /// 構築時に渡された設定をそのまま返す。
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

impl AudioCaptureSession<'_> {
    /// キャプチャを停止する。
    ///
    /// 実行中のユーザーコールバックがある場合は、その完了を待ってから戻る。
    /// この関数が戻った後にこのセッションのユーザーコールバックが呼ばれることはない。
    pub fn stop(self) {
        drop(self);
    }

    /// [`AudioCapture::config`] と同じ。
    pub fn config(&self) -> &AudioCaptureConfig {
        self.capture.config()
    }

    /// [`AudioCapture::sample_rate`] と同じ。
    pub fn sample_rate(&self) -> i32 {
        self.capture.sample_rate()
    }

    /// [`AudioCapture::channels`] と同じ。
    pub fn channels(&self) -> i32 {
        self.capture.channels()
    }
}

impl Drop for AudioCaptureSession<'_> {
    fn drop(&mut self) {
        // 停止忘れを防ぐため、ここでセッションを停止する
        self.capture.stop_session();
    }
}
