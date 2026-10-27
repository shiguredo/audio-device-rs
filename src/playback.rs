//! オーディオ再生の定義。
//!
//! バックエンドに依存しない形で再生の開始・停止・設定取得を提供する
//! [AudioPlayback] と [AudioPlaybackSession] を定義する。

use crate::common::{AudioPlaybackConfig, PlaybackFrame};
use crate::error::Result;

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
use crate::playback_ffi::FfiPlaybackImpl;

#[cfg(enable_wasapi)]
use crate::playback_wasapi::WasapiPlaybackImpl;

/// オーディオ再生。
///
/// 再生を開始するには [AudioPlayback::start] を呼ぶ。
pub struct AudioPlayback(AudioPlaybackInner);

/// 実行中のオーディオ再生セッション。
///
/// [AudioPlayback::start] が返す。この値が生存している間は [AudioPlayback] を
/// 借用する。停止は [AudioPlaybackSession::stop] で行い、`stop` を呼ばずに
/// drop した場合も停止する。Drop は実行中のユーザーコールバックの完了を待つ。
/// ガードは別のスレッドへ move できる。
#[must_use = "dropping the session stops the playback"]
pub struct AudioPlaybackSession<'a> {
    /// セッションを開始した再生
    playback: &'a mut AudioPlayback,
}

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
    ///
    /// 開始したセッションを表す [AudioPlaybackSession] を返す。返されたガードが
    /// 生存している間はこの `AudioPlayback` を借用するため、再度 `start` を
    /// 呼ぶことも、`AudioPlayback` を drop することもできない。
    ///
    /// # Errors
    ///
    /// 開始に失敗した場合に `Error::SessionStartFailed` を返す。
    ///
    /// # セッション中の借用
    ///
    /// 次のコードは、セッションが `AudioPlayback` を借用している間に再度
    /// `start` を呼ぶためコンパイルエラーになる。
    ///
    /// ```compile_fail,E0499
    /// fn cannot_start_twice(mut playback: shiguredo_audio_device::AudioPlayback) {
    ///     let session = playback.start().expect("playback must start");
    ///     let _ = playback.start();
    ///     drop(session);
    /// }
    /// ```
    ///
    /// 次のコードは、セッションが `AudioPlayback` を借用している間に
    /// `AudioPlayback` を drop するためコンパイルエラーになる。
    ///
    /// ```compile_fail,E0505
    /// fn cannot_drop_while_running(mut playback: shiguredo_audio_device::AudioPlayback) {
    ///     let session = playback.start().expect("playback must start");
    ///     drop(playback);
    ///     drop(session);
    /// }
    /// ```
    pub fn start(&mut self) -> Result<AudioPlaybackSession<'_>> {
        match &mut self.0 {
            #[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
            AudioPlaybackInner::Ffi(inner) => inner.start()?,
            #[cfg(enable_wasapi)]
            AudioPlaybackInner::Wasapi(inner) => inner.start()?,
        }

        Ok(AudioPlaybackSession { playback: self })
    }

    /// 実行中のセッションを停止する。
    fn stop_session(&mut self) {
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

impl AudioPlaybackSession<'_> {
    /// 再生を停止する。
    ///
    /// 実行中のユーザーコールバックがある場合は、その完了を待ってから戻る。
    /// 戻った後にこのセッションのユーザーコールバックが呼ばれることはない。
    pub fn stop(self) {
        // Drop に停止処理を委譲する
        drop(self);
    }

    /// [`AudioPlayback::config`] と同じ。
    pub fn config(&self) -> &AudioPlaybackConfig {
        self.playback.config()
    }

    /// [`AudioPlayback::sample_rate`] と同じ。
    pub fn sample_rate(&self) -> i32 {
        self.playback.sample_rate()
    }

    /// [`AudioPlayback::channels`] と同じ。
    pub fn channels(&self) -> i32 {
        self.playback.channels()
    }
}

impl Drop for AudioPlaybackSession<'_> {
    fn drop(&mut self) {
        // 停止忘れを防ぐため、ここでセッションを停止する
        self.playback.stop_session();
    }
}
