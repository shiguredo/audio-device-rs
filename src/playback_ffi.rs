//! macOS / Linux 共通のオーディオ再生実装。
//!
//! バックエンドごとに FFI 関数群を [PlaybackOps] で渡し、[FfiPlaybackImpl] が
//! 再生の実装を一括で提供する。

use std::ffi::{CString, c_char, c_void};
use std::ptr::NonNull;

use crate::common::{
    AudioFormat, AudioPlaybackConfig, PlaybackFrame, write_playback_frame_to_buffer,
};
use crate::error::{Error, Result};
use crate::ffi;

// ---------------------------------------------------------------------------
// バックエンドとの境界
// ---------------------------------------------------------------------------

/// バックエンド固有の FFI 関数テーブル。
struct PlaybackOps {
    session_create: unsafe extern "C" fn(
        device_id: *const c_char,
        sample_rate: i32,
        channels: i32,
    ) -> *mut ffi::PlaybackSession,
    session_start: unsafe extern "C" fn(
        session: *mut ffi::PlaybackSession,
        callback: ffi::AudioPlaybackCallback,
        context: *mut c_void,
    ) -> i32,
    session_stop: unsafe extern "C" fn(session: *mut ffi::PlaybackSession),
    session_destroy: unsafe extern "C" fn(session: *mut ffi::PlaybackSession),
    session_sample_rate: unsafe extern "C" fn(session: *mut ffi::PlaybackSession) -> i32,
    session_channels: unsafe extern "C" fn(session: *mut ffi::PlaybackSession) -> i32,
}

// ---------------------------------------------------------------------------
// コールバックコンテキスト
// ---------------------------------------------------------------------------

pub(crate) struct PlaybackContext {
    callback: Box<dyn Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync>,
}

// ---------------------------------------------------------------------------
// 汎用再生実装
// ---------------------------------------------------------------------------

pub(crate) struct FfiPlaybackImpl {
    /// バックエンド固有の FFI 関数群
    ops: &'static PlaybackOps,
    /// C 側の再生セッション。destroy 後は None になる
    session: Option<NonNull<ffi::PlaybackSession>>,
    /// ユーザーコールバックを保持するコンテキスト
    context: Option<Box<PlaybackContext>>,
    /// ユーザーが指定した再生設定
    config: AudioPlaybackConfig,
    /// C 側とネゴシエーションされた実際のサンプルレート
    actual_sample_rate: i32,
    /// C 側とネゴシエーションされた実際のチャンネル数
    actual_channels: i32,
    /// 再生中かどうか（C 側の running とは独立に管理）
    running: bool,
}

impl FfiPlaybackImpl {
    fn new(
        ops: &'static PlaybackOps,
        config: AudioPlaybackConfig,
        callback: impl Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    ) -> Result<Self> {
        let device_id_cstr = config.device_id.as_ref().map(|s| CString::new(s.as_str()));
        let device_id_ptr = match &device_id_cstr {
            Some(Ok(cstr)) => cstr.as_ptr(),
            Some(Err(_)) => {
                return Err(Error::NullPointer("device_id contains null byte"));
            }
            None => std::ptr::null(),
        };

        let session =
            unsafe { (ops.session_create)(device_id_ptr, config.sample_rate, config.channels) };
        let session = NonNull::new(session).ok_or(Error::SessionCreateFailed)?;

        let actual_sample_rate = unsafe { (ops.session_sample_rate)(session.as_ptr()) };
        let actual_channels = unsafe { (ops.session_channels)(session.as_ptr()) };

        let context = Box::new(PlaybackContext {
            callback: Box::new(callback),
        });

        Ok(Self {
            ops,
            session: Some(session),
            context: Some(context),
            config,
            actual_sample_rate,
            actual_channels,
            running: false,
        })
    }

    #[cfg(enable_coreaudio)]
    pub(crate) fn new_coreaudio<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Self::new(&OPS_COREAUDIO, config, callback)
    }

    #[cfg(enable_pulse)]
    pub(crate) fn new_pulse<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Self::new(&OPS_PULSE, config, callback)
    }

    #[cfg(enable_pipewire)]
    pub(crate) fn new_pipewire<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Self::new(&OPS_PIPEWIRE, config, callback)
    }

    pub fn start(&mut self) -> Result<()> {
        let session = self.session.ok_or(Error::SessionStartFailed)?;
        let context = self.context.as_mut().ok_or(Error::SessionStartFailed)?;

        if self.running {
            return Ok(());
        }

        // 再生用コールバックコンテキストのポインタを C 側に渡す。
        // FfiPlaybackImpl の生存期間中は Box<PlaybackContext> が有効であるため、
        // C 側からのコールバックは安全に参照できる。
        let context_ptr = &mut **context as *mut PlaybackContext as *mut c_void;

        let ret = unsafe {
            (self.ops.session_start)(session.as_ptr(), Some(playback_callback), context_ptr)
        };
        if ret < 0 {
            return Err(Error::SessionStartFailed);
        }

        self.running = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        if self.running {
            if let Some(session) = self.session {
                unsafe { (self.ops.session_stop)(session.as_ptr()) };
            }
            self.running = false;
        }
    }

    pub fn config(&self) -> &AudioPlaybackConfig {
        &self.config
    }

    pub fn sample_rate(&self) -> i32 {
        self.actual_sample_rate
    }

    pub fn channels(&self) -> i32 {
        self.actual_channels
    }
}

impl Drop for FfiPlaybackImpl {
    fn drop(&mut self) {
        self.stop();
        if let Some(session) = self.session.take() {
            unsafe { (self.ops.session_destroy)(session.as_ptr()) };
        }
    }
}

// SAFETY: 内部に保持する FFI セッションポインタは C 側のスレッド安全性に従い、
// PlaybackContext のコールバックは Box<dyn Fn + Send + Sync> でスレッド安全。
unsafe impl Send for FfiPlaybackImpl {}

// ---------------------------------------------------------------------------
// extern "C" 再生コールバック（全バックエンド共通）
// ---------------------------------------------------------------------------

extern "C" fn playback_callback(
    user_data: *mut c_void,
    buffer: *mut c_void,
    frames: i32,
    channels: i32,
    sample_rate: i32,
    format: i32,
) -> i32 {
    // 引数の妥当性を検証する
    if user_data.is_null() || buffer.is_null() || frames <= 0 || channels <= 0 || sample_rate <= 0 {
        return 0;
    }

    let context = unsafe { &*(user_data as *const PlaybackContext) };

    // C 側から渡されたフォーマット定数を Rust 側の型に変換する
    let audio_format = match AudioFormat::from_ffi(format) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let bytes_per_sample: usize = match audio_format {
        AudioFormat::S16 => 2,
        AudioFormat::F32 => 4,
    };

    // バッファサイズを安全に計算する
    let Some(buffer_size) = (frames as usize)
        .checked_mul(channels as usize)
        .and_then(|n| n.checked_mul(bytes_per_sample))
    else {
        return 0;
    };

    // ユーザーコールバックからフレームデータを取得する
    let Some(frame) = (context.callback)(frames, channels, sample_rate) else {
        return 0;
    };

    // フレームデータをバッファに変換して書き込む
    let dst = unsafe { std::slice::from_raw_parts_mut(buffer as *mut u8, buffer_size) };
    write_playback_frame_to_buffer(
        &frame.data,
        frame.format,
        dst,
        audio_format,
        channels as usize,
    )
}

// ---------------------------------------------------------------------------
// バックエンド別 OPS 定数
// ---------------------------------------------------------------------------

#[cfg(enable_coreaudio)]
const OPS_COREAUDIO: PlaybackOps = PlaybackOps {
    session_create: ffi::audio_coreaudio_playback_session_create,
    session_start: ffi::audio_coreaudio_playback_session_start,
    session_stop: ffi::audio_coreaudio_playback_session_stop,
    session_destroy: ffi::audio_coreaudio_playback_session_destroy,
    session_sample_rate: ffi::audio_coreaudio_playback_session_sample_rate,
    session_channels: ffi::audio_coreaudio_playback_session_channels,
};

#[cfg(enable_pulse)]
const OPS_PULSE: PlaybackOps = PlaybackOps {
    session_create: ffi::audio_pulse_playback_session_create,
    session_start: ffi::audio_pulse_playback_session_start,
    session_stop: ffi::audio_pulse_playback_session_stop,
    session_destroy: ffi::audio_pulse_playback_session_destroy,
    session_sample_rate: ffi::audio_pulse_playback_session_sample_rate,
    session_channels: ffi::audio_pulse_playback_session_channels,
};

#[cfg(enable_pipewire)]
const OPS_PIPEWIRE: PlaybackOps = PlaybackOps {
    session_create: ffi::audio_pipewire_playback_session_create,
    session_start: ffi::audio_pipewire_playback_session_start,
    session_stop: ffi::audio_pipewire_playback_session_stop,
    session_destroy: ffi::audio_pipewire_playback_session_destroy,
    session_sample_rate: ffi::audio_pipewire_playback_session_sample_rate,
    session_channels: ffi::audio_pipewire_playback_session_channels,
};
