use std::ffi::CString;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::common::{AudioCaptureConfig, AudioFormat, AudioFrame, CaptureContext};
use crate::error::{Error, Result};
use crate::ffi;

pub struct AudioCapture {
    session: Option<NonNull<ffi::AudioSession>>,
    context: Option<Arc<CaptureContext>>,
    config: AudioCaptureConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
}

impl AudioCapture {
    pub fn new<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        let device_id_cstr = config.device_id.as_ref().map(|s| CString::new(s.as_str()));
        let device_id_ptr = match &device_id_cstr {
            Some(Ok(cstr)) => cstr.as_ptr(),
            Some(Err(_)) => return Err(Error::NullPointer("device_id contains null byte")),
            None => std::ptr::null(),
        };

        let session = unsafe {
            ffi::audio_session_create(device_id_ptr, config.sample_rate, config.channels)
        };

        let session = NonNull::new(session).ok_or(Error::SessionCreateFailed)?;

        // 実際のサンプルレートとチャンネル数を取得
        let actual_sample_rate = unsafe { ffi::audio_session_sample_rate(session.as_ptr()) };
        let actual_channels = unsafe { ffi::audio_session_channels(session.as_ptr()) };

        let context = Arc::new(CaptureContext {
            callback: Box::new(callback),
            running: AtomicBool::new(false),
        });

        Ok(Self {
            session: Some(session),
            context: Some(context),
            config,
            actual_sample_rate,
            actual_channels,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        let session = self.session.ok_or(Error::SessionStartFailed)?;
        let context = self.context.as_ref().ok_or(Error::SessionStartFailed)?;

        if context.running.load(Ordering::Acquire) {
            return Ok(());
        }

        let context_ptr = Arc::as_ptr(context) as *mut std::ffi::c_void;
        let ret = unsafe {
            ffi::audio_session_start(session.as_ptr(), Some(frame_callback), context_ptr)
        };

        if ret < 0 {
            return Err(Error::SessionStartFailed);
        }

        context.running.store(true, Ordering::Release);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(context) = &self.context
            && context.running.load(Ordering::Acquire)
        {
            if let Some(session) = self.session {
                unsafe { ffi::audio_session_stop(session.as_ptr()) };
            }
            context.running.store(false, Ordering::Release);
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

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
        if let Some(session) = self.session.take() {
            unsafe { ffi::audio_session_destroy(session.as_ptr()) };
        }
    }
}

// AudioCapture はプラットフォーム固有のキャプチャセッションを内部で管理し、
// コールバックはスレッドセーフな Arc<CaptureContext> を通じて処理される
unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

extern "C" fn frame_callback(
    user_data: *mut std::ffi::c_void,
    data: *const std::ffi::c_void,
    frames: i32,
    channels: i32,
    sample_rate: i32,
    format: i32,
    timestamp_us: i64,
) {
    if user_data.is_null() || data.is_null() || frames <= 0 || channels <= 0 {
        return;
    }

    // SAFETY: user_data は Arc<CaptureContext> から取得したポインタ
    // context の生存期間は AudioCapture によって保証される
    let context = unsafe { &*(user_data as *const CaptureContext) };

    let audio_format = match AudioFormat::from_ffi(format) {
        Ok(f) => f,
        Err(_) => return,
    };
    let bytes_per_sample: usize = match audio_format {
        AudioFormat::S16 => 2,
        AudioFormat::F32 => 4,
    };
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

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (context.callback)(frame);
    }));
}
