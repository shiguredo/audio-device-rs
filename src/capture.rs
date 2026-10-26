//! オーディオキャプチャの定義。
//!
//! バックエンドに依存しない形でキャプチャの開始・停止・設定取得を提供する
//! [AudioCapture] と [AudioCaptureSession] を提供する。

use crate::common::{AudioCaptureConfig, AudioFrame};
use crate::error::Result;

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
use crate::capture_ffi::FfiCaptureImpl;

#[cfg(enable_wasapi)]
use crate::capture_wasapi::WasapiCaptureImpl;

/// オーディオキャプチャ。
///
/// キャプチャを開始するには [AudioCapture::start] を呼ぶ。開始すると
/// [AudioCaptureSession] が返り、そのガードが生存している間はこの値を借用する。
/// そのため、セッション実行中にこの値を drop したり、再度 `start` を
/// 呼んだりすることはできない。
pub struct AudioCapture(AudioCaptureInner);

/// 実行中のオーディオキャプチャセッション。
///
/// [AudioCapture::start] が返す。この値が生存している間は [AudioCapture] を
/// 借用するため、`AudioCapture` のメソッドを呼ぶことも drop することもできない。
/// セッション中の設定や実際のサンプルレート・チャンネル数は、[AudioCaptureSession::config] /
/// [AudioCaptureSession::sample_rate] / [AudioCaptureSession::channels] から取得する。
///
/// 停止するには [AudioCaptureSession::stop] を呼ぶ。`stop` は実行中のユーザー
/// コールバックの完了を待ってから戻る。`stop` を呼ばずに drop した場合も停止し、
/// 実行中のユーザーコールバックの完了を待つ。ガードは別のスレッドへ move できる
/// ため、そのスレッドから `stop` を呼ぶことで停止できる。
#[must_use = "dropping the session stops the capture immediately"]
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

    /// CoreAudio でキャプチャを構築する。
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

    /// PulseAudio でキャプチャを構築する。
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

    /// PipeWire でキャプチャを構築する。
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

    /// WASAPI でキャプチャを構築する。
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
    /// ```compile_fail
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
    /// ```compile_fail
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
    /// 戻った後にこのセッションのユーザーコールバックが呼ばれることはない。
    ///
    /// 別のスレッドから呼ぶ場合は、そのスレッドで `initialize_thread()` を呼び、
    /// 返り値をスレッドの終了まで保持すること。
    pub fn stop(self) {
        self.capture.stop_session();
    }

    /// キャプチャ設定を取得する。
    ///
    /// 構築時に渡された設定をそのまま返す。
    pub fn config(&self) -> &AudioCaptureConfig {
        self.capture.config()
    }

    /// 実際のサンプルレートを取得する。
    pub fn sample_rate(&self) -> i32 {
        self.capture.sample_rate()
    }

    /// 実際のチャンネル数を取得する。
    pub fn channels(&self) -> i32 {
        self.capture.channels()
    }
}

impl Drop for AudioCaptureSession<'_> {
    fn drop(&mut self) {
        // stop 済みでも各バックエンドの stop は冪等なので、停止忘れを防ぐため常に呼ぶ
        self.capture.stop_session();
    }
}
