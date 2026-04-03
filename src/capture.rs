use std::ffi::CString;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::device::AudioFormat;
use crate::error::{Error, Result};
use crate::ffi;

/// オーディオフレームデータ
pub struct AudioFrame<'a> {
    /// PCM データ
    pub data: &'a [u8],
    /// サンプルフレーム数
    pub frames: i32,
    /// チャンネル数
    pub channels: i32,
    /// サンプルレート
    pub sample_rate: i32,
    /// オーディオフォーマット
    pub format: AudioFormat,
    /// タイムスタンプ（マイクロ秒）
    pub timestamp_us: i64,
}

impl<'a> AudioFrame<'a> {
    /// 参照データを所有データに変換する
    pub fn to_owned(&self) -> AudioFrameOwned {
        AudioFrameOwned {
            data: self.data.to_vec(),
            frames: self.frames,
            channels: self.channels,
            sample_rate: self.sample_rate,
            format: self.format,
            timestamp_us: self.timestamp_us,
        }
    }

    /// S16 フォーマットとしてデータを取得
    pub fn as_s16(&self) -> Option<&[i16]> {
        if self.format != AudioFormat::S16 {
            return None;
        }
        if self.frames <= 0 || self.channels <= 0 {
            return None;
        }
        let len = (self.frames as usize) * (self.channels as usize);
        let required_bytes = len * std::mem::size_of::<i16>();
        if self.data.len() < required_bytes {
            return None;
        }
        if !(self.data.as_ptr() as usize).is_multiple_of(std::mem::align_of::<i16>()) {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(self.data.as_ptr() as *const i16, len) })
    }

    /// F32 フォーマットとしてデータを取得
    pub fn as_f32(&self) -> Option<&[f32]> {
        if self.format != AudioFormat::F32 {
            return None;
        }
        if self.frames <= 0 || self.channels <= 0 {
            return None;
        }
        let len = (self.frames as usize) * (self.channels as usize);
        let required_bytes = len * std::mem::size_of::<f32>();
        if self.data.len() < required_bytes {
            return None;
        }
        if !(self.data.as_ptr() as usize).is_multiple_of(std::mem::align_of::<f32>()) {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(self.data.as_ptr() as *const f32, len) })
    }
}

/// オーディオフレームデータ (所有)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrameOwned {
    /// PCM データ
    pub data: Vec<u8>,
    /// サンプルフレーム数
    pub frames: i32,
    /// チャンネル数
    pub channels: i32,
    /// サンプルレート
    pub sample_rate: i32,
    /// オーディオフォーマット
    pub format: AudioFormat,
    /// タイムスタンプ（マイクロ秒）
    pub timestamp_us: i64,
}

impl AudioFrameOwned {
    /// 参照フレームとして取得する
    pub fn as_frame(&self) -> AudioFrame<'_> {
        AudioFrame {
            data: &self.data,
            frames: self.frames,
            channels: self.channels,
            sample_rate: self.sample_rate,
            format: self.format,
            timestamp_us: self.timestamp_us,
        }
    }

    /// S16 フォーマットとしてデータを取得
    pub fn as_s16(&self) -> Option<&[i16]> {
        self.as_frame().as_s16().map(|s| {
            // SAFETY: as_frame() は self.data を参照しており、self の借用中は有効
            unsafe { std::slice::from_raw_parts(s.as_ptr(), s.len()) }
        })
    }

    /// F32 フォーマットとしてデータを取得
    pub fn as_f32(&self) -> Option<&[f32]> {
        self.as_frame().as_f32().map(|s| {
            // SAFETY: as_frame() は self.data を参照しており、self の借用中は有効
            unsafe { std::slice::from_raw_parts(s.as_ptr(), s.len()) }
        })
    }
}

pub struct AudioCaptureConfig {
    pub device_id: Option<String>,
    pub sample_rate: i32,
    pub channels: i32,
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            sample_rate: 48000,
            channels: 1,
        }
    }
}

struct CaptureContext {
    callback: Box<dyn Fn(AudioFrame<'_>) + Send + Sync>,
    running: AtomicBool,
}

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

    let audio_format = AudioFormat::from_ffi(format);
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
