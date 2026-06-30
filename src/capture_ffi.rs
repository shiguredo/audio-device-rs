//! macOS / Linux 共通のオーディオキャプチャ実装。
//!
//! バックエンドごとに FFI 関数群を [CaptureOps] で渡し、[FfiCaptureImpl] が
//! キャプチャの実装を一括で提供する。

use std::ffi::{CString, c_char, c_void};
use std::ptr::NonNull;

use crate::common::{AudioCaptureConfig, AudioFormat, AudioFrame};
use crate::error::{Error, Result};
use crate::ffi;

// ---------------------------------------------------------------------------
// バックエンドとの境界
// ---------------------------------------------------------------------------

/// バックエンド固有の FFI 関数テーブル。
struct CaptureOps {
    session_create: unsafe extern "C" fn(
        device_id: *const c_char,
        sample_rate: i32,
        channels: i32,
    ) -> *mut ffi::AudioSession,
    session_start: unsafe extern "C" fn(
        session: *mut ffi::AudioSession,
        callback: ffi::AudioFrameCallback,
        context: *mut c_void,
    ) -> i32,
    session_stop: unsafe extern "C" fn(session: *mut ffi::AudioSession),
    session_destroy: unsafe extern "C" fn(session: *mut ffi::AudioSession),
    session_sample_rate: unsafe extern "C" fn(session: *mut ffi::AudioSession) -> i32,
    session_channels: unsafe extern "C" fn(session: *mut ffi::AudioSession) -> i32,
}

// ---------------------------------------------------------------------------
// コールバックコンテキスト
// ---------------------------------------------------------------------------

pub(crate) struct CaptureContext {
    callback: Box<dyn Fn(AudioFrame<'_>) + Send + Sync>,
}

// ---------------------------------------------------------------------------
// 汎用キャプチャ実装
// ---------------------------------------------------------------------------

pub(crate) struct FfiCaptureImpl {
    ops: &'static CaptureOps,
    session: Option<NonNull<ffi::AudioSession>>,
    context: Box<CaptureContext>,
    config: AudioCaptureConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
    running: bool,
}

impl FfiCaptureImpl {
    fn new(
        ops: &'static CaptureOps,
        config: AudioCaptureConfig,
        callback: impl Fn(AudioFrame<'_>) + Send + Sync + 'static,
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

        let context = Box::new(CaptureContext {
            callback: Box::new(callback),
        });

        Ok(Self {
            ops,
            session: Some(session),
            context,
            config,
            actual_sample_rate,
            actual_channels,
            running: false,
        })
    }

    #[cfg(enable_coreaudio)]
    pub(crate) fn new_coreaudio<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        Self::new(&OPS_COREAUDIO, config, callback)
    }

    #[cfg(enable_pulse)]
    pub(crate) fn new_pulse<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        Self::new(&OPS_PULSE, config, callback)
    }

    #[cfg(enable_pipewire)]
    pub(crate) fn new_pipewire<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        Self::new(&OPS_PIPEWIRE, config, callback)
    }

    pub fn start(&mut self) -> Result<()> {
        let session = self.session.ok_or(Error::SessionStartFailed)?;
        let context = &mut *self.context;

        if self.running {
            return Ok(());
        }

        let context_ptr = context as *mut CaptureContext as *mut c_void;

        let ret = unsafe {
            (self.ops.session_start)(session.as_ptr(), Some(frame_callback), context_ptr)
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

    pub fn config(&self) -> &AudioCaptureConfig {
        &self.config
    }

    pub fn sample_rate(&self) -> i32 {
        self.actual_sample_rate
    }

    pub fn channels(&self) -> i32 {
        self.actual_channels
    }
}

impl Drop for FfiCaptureImpl {
    fn drop(&mut self) {
        self.stop();
        if let Some(session) = self.session.take() {
            unsafe { (self.ops.session_destroy)(session.as_ptr()) };
        }
    }
}

// SAFETY: 内部に保持する FFI セッションポインタは C 側のスレッド安全性に従い、
// CaptureContext のコールバックは Box<dyn Fn + Send + Sync> でスレッド安全。
unsafe impl Send for FfiCaptureImpl {}

// ---------------------------------------------------------------------------
// extern "C" フレームコールバック（全バックエンド共通）
// ---------------------------------------------------------------------------

extern "C" fn frame_callback(
    user_data: *mut c_void,
    data: *const c_void,
    frames: i32,
    channels: i32,
    sample_rate: i32,
    format: i32,
    timestamp_us: i64,
) {
    if user_data.is_null() || data.is_null() || frames <= 0 || channels <= 0 {
        return;
    }

    let context = unsafe { &*(user_data as *const CaptureContext) };

    let audio_format = match AudioFormat::from_ffi(format) {
        Ok(f) => f,
        Err(_) => return,
    };

    let bytes_per_sample: usize = audio_format.bytes_per_sample();

    let Some(data_size) = (frames as usize)
        .checked_mul(channels as usize)
        .and_then(|n| n.checked_mul(bytes_per_sample))
    else {
        return;
    };

    let data_slice = unsafe { std::slice::from_raw_parts(data as *const u8, data_size) };

    let frame = AudioFrame {
        data: data_slice,
        frames,
        channels,
        sample_rate,
        format: audio_format,
        timestamp_us,
    };

    (context.callback)(frame);
}

// ---------------------------------------------------------------------------
// バックエンド別 OPS 定数
// ---------------------------------------------------------------------------

#[cfg(enable_coreaudio)]
const OPS_COREAUDIO: CaptureOps = CaptureOps {
    session_create: ffi::audio_coreaudio_session_create,
    session_start: ffi::audio_coreaudio_session_start,
    session_stop: ffi::audio_coreaudio_session_stop,
    session_destroy: ffi::audio_coreaudio_session_destroy,
    session_sample_rate: ffi::audio_coreaudio_session_sample_rate,
    session_channels: ffi::audio_coreaudio_session_channels,
};

#[cfg(enable_pulse)]
const OPS_PULSE: CaptureOps = CaptureOps {
    session_create: ffi::audio_pulse_session_create,
    session_start: ffi::audio_pulse_session_start,
    session_stop: ffi::audio_pulse_session_stop,
    session_destroy: ffi::audio_pulse_session_destroy,
    session_sample_rate: ffi::audio_pulse_session_sample_rate,
    session_channels: ffi::audio_pulse_session_channels,
};

#[cfg(enable_pipewire)]
const OPS_PIPEWIRE: CaptureOps = CaptureOps {
    session_create: ffi::audio_pipewire_session_create,
    session_start: ffi::audio_pipewire_session_start,
    session_stop: ffi::audio_pipewire_session_stop,
    session_destroy: ffi::audio_pipewire_session_destroy,
    session_sample_rate: ffi::audio_pipewire_session_sample_rate,
    session_channels: ffi::audio_pipewire_session_channels,
};
