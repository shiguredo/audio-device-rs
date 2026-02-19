//! macOS/Linux 用オーディオ再生

use std::ffi::CString;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::common::{AudioFormat, AudioPlaybackConfig, PlaybackFrame};
use crate::error::{Error, Result};
use crate::ffi;

struct PlaybackContext {
    callback: Box<dyn Fn() -> Option<PlaybackFrame> + Send + Sync>,
}
/// オーディオ再生
pub struct AudioPlayback {
    session: Option<NonNull<ffi::PlaybackSession>>,
    context: Option<Arc<PlaybackContext>>,
    config: AudioPlaybackConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
}

impl AudioPlayback {
    /// 新しい AudioPlayback を作成
    ///
    /// `callback` はフレームデータを要求されたときに呼ばれる。
    /// データがない場合は `None` を返すと無音が再生される。
    pub fn new<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn() -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        let device_id_cstr = config.device_id.as_ref().map(|s| CString::new(s.as_str()));
        let device_id_ptr = match &device_id_cstr {
            Some(Ok(cstr)) => cstr.as_ptr(),
            Some(Err(_)) => return Err(Error::NullPointer("device_id contains null byte")),
            None => std::ptr::null(),
        };

        let session = unsafe {
            ffi::playback_session_create(device_id_ptr, config.sample_rate, config.channels)
        };

        let session = NonNull::new(session).ok_or(Error::SessionCreateFailed)?;

        let actual_sample_rate = unsafe { ffi::playback_session_sample_rate(session.as_ptr()) };
        let actual_channels = unsafe { ffi::playback_session_channels(session.as_ptr()) };

        let context = Arc::new(PlaybackContext {
            callback: Box::new(callback),
        });

        Ok(Self {
            session: Some(session),
            context: Some(context),
            config,
            actual_sample_rate,
            actual_channels,
        })
    }

    /// 再生を開始
    pub fn start(&mut self) -> Result<()> {
        let session = self.session.ok_or(Error::SessionStartFailed)?;
        let context = self.context.as_ref().ok_or(Error::SessionStartFailed)?;

        let context_ptr = Arc::as_ptr(context) as *mut std::ffi::c_void;
        let ret = unsafe {
            ffi::playback_session_start(session.as_ptr(), Some(playback_callback), context_ptr)
        };

        if ret < 0 {
            return Err(Error::SessionStartFailed);
        }

        Ok(())
    }

    /// 再生を停止
    pub fn stop(&mut self) {
        if let Some(session) = self.session {
            unsafe { ffi::playback_session_stop(session.as_ptr()) };
        }
    }

    /// 設定を取得
    pub fn config(&self) -> &AudioPlaybackConfig {
        &self.config
    }

    /// 実際のサンプルレートを取得
    pub fn sample_rate(&self) -> i32 {
        self.actual_sample_rate
    }

    /// 実際のチャンネル数を取得
    pub fn channels(&self) -> i32 {
        self.actual_channels
    }
}

impl Drop for AudioPlayback {
    fn drop(&mut self) {
        self.stop();
        if let Some(session) = self.session.take() {
            unsafe { ffi::playback_session_destroy(session.as_ptr()) };
        }
    }
}

// AudioPlayback はプラットフォーム固有の再生セッションを内部で管理し、
// コールバックはスレッドセーフな Arc<PlaybackContext> を通じて処理される
unsafe impl Send for AudioPlayback {}

extern "C" fn playback_callback(
    user_data: *mut std::ffi::c_void,
    buffer: *mut std::ffi::c_void,
    frames: i32,
    channels: i32,
    _sample_rate: i32, // FFI シグネチャ上必要だが Rust 側ではフォーマット変換に不要
    format: i32,
) -> i32 {
    if user_data.is_null() || buffer.is_null() || frames <= 0 {
        return 0;
    }

    // SAFETY: user_data は Arc<PlaybackContext> から取得したポインタ
    // context の生存期間は AudioPlayback によって保証される
    let context = unsafe { &*(user_data as *const PlaybackContext) };

    let frame_opt = (context.callback)();

    let Some(frame) = frame_opt else {
        return 0;
    };

    let audio_format = AudioFormat::from_ffi(format);
    let bytes_per_sample = match audio_format {
        AudioFormat::S16 => 2,
        AudioFormat::F32 => 4,
    };
    let buffer_size = (frames * channels * bytes_per_sample) as usize;

    // フレームデータをバッファにコピー
    if frame.format == audio_format {
        let copy_len = frame.data.len().min(buffer_size);
        unsafe {
            std::ptr::copy_nonoverlapping(frame.data.as_ptr(), buffer as *mut u8, copy_len);
        }
        copy_len as i32 / (channels * bytes_per_sample)
    } else if frame.format == AudioFormat::F32 && audio_format == AudioFormat::S16 {
        // F32 -> S16 変換
        let src = unsafe {
            std::slice::from_raw_parts(frame.data.as_ptr() as *const f32, frame.data.len() / 4)
        };
        let dst = unsafe { std::slice::from_raw_parts_mut(buffer as *mut i16, buffer_size / 2) };
        let copy_len = src.len().min(dst.len());
        for i in 0..copy_len {
            dst[i] = (src[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
        }
        copy_len as i32 / channels
    } else if frame.format == AudioFormat::S16 && audio_format == AudioFormat::F32 {
        // S16 -> F32 変換
        let src = unsafe {
            std::slice::from_raw_parts(frame.data.as_ptr() as *const i16, frame.data.len() / 2)
        };
        let dst = unsafe { std::slice::from_raw_parts_mut(buffer as *mut f32, buffer_size / 4) };
        let copy_len = src.len().min(dst.len());
        for i in 0..copy_len {
            dst[i] = src[i] as f32 / 32768.0;
        }
        copy_len as i32 / channels
    } else {
        0
    }
}
