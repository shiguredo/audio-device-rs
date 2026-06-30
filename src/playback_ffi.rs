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
    let bytes_per_sample: usize = audio_format.bytes_per_sample();

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

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の PlaybackContext を作成する
    fn make_context(
        cb: impl Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + Sync + 'static,
    ) -> Box<PlaybackContext> {
        Box::new(PlaybackContext {
            callback: Box::new(cb),
        })
    }

    /// テスト用の PlaybackContext のポインタを取得する
    fn context_ptr(ctx: &PlaybackContext) -> *mut c_void {
        ctx as *const PlaybackContext as *mut c_void
    }

    // -----------------------------------------------------------------------
    // NULL 引数・境界値テスト
    // -----------------------------------------------------------------------

    #[test]
    fn returns_zero_when_user_data_is_null() {
        let mut buf = vec![0u8; 480];
        let ret = playback_callback(
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            480,
            1,
            48000,
            0,
        );
        assert_eq!(ret, 0);
    }

    #[test]
    fn returns_zero_when_buffer_is_null() {
        let ctx = make_context(|_, _, _| None);
        let ret = playback_callback(context_ptr(&ctx), std::ptr::null_mut(), 480, 1, 48000, 0);
        assert_eq!(ret, 0);
    }

    #[test]
    fn returns_zero_when_frames_is_zero() {
        let ctx = make_context(|_, _, _| None);
        let mut buf = vec![0u8; 480];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            0,
            1,
            48000,
            0,
        );
        assert_eq!(ret, 0);
    }

    #[test]
    fn returns_zero_when_channels_is_zero() {
        let ctx = make_context(|_, _, _| None);
        let mut buf = vec![0u8; 480];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            480,
            0,
            48000,
            0,
        );
        assert_eq!(ret, 0);
    }

    #[test]
    fn returns_zero_when_sample_rate_is_zero() {
        let ctx = make_context(|_, _, _| None);
        let mut buf = vec![0u8; 480];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            480,
            1,
            0,
            0,
        );
        assert_eq!(ret, 0);
    }

    #[test]
    fn returns_zero_when_format_is_unknown() {
        let mut buf = vec![0u8; 480];
        let ctx = make_context(|frames, channels, sample_rate| {
            Some(
                PlaybackFrame::from_s16(
                    &vec![0i16; (frames * channels) as usize],
                    channels,
                    sample_rate,
                )
                .expect("S16 フレーム作成に成功する"),
            )
        });
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            240,
            1,
            48000,
            99,
        );
        assert_eq!(ret, 0);
    }

    // -----------------------------------------------------------------------
    // コールバックが None を返す場合
    // -----------------------------------------------------------------------

    #[test]
    fn fills_buffer_with_silence_when_callback_returns_none() {
        let ctx = make_context(|_, _, _| None);
        let mut buf = vec![0xCCu8; 480];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            240,
            2,
            48000,
            0,
        );
        assert_eq!(ret, 0);
        // コールバックが None の場合は 0 を返す（無音の埋め込みは C 側の責務）
    }

    // -----------------------------------------------------------------------
    // 同一フォーマット (S16 → S16)
    // -----------------------------------------------------------------------

    #[test]
    fn same_format_s16_copies_samples() {
        let channels = 2;
        let frames = 10;
        let sample_rate = 48000;
        let samples: Vec<i16> = (0..(frames * channels) as i16).collect();
        let ctx = make_context({
            let samples = samples.clone();
            move |f, c, sr| {
                assert_eq!(f, frames);
                assert_eq!(c, channels);
                assert_eq!(sr, sample_rate);
                Some(PlaybackFrame::from_s16(&samples, c, sr).expect("S16 フレーム作成に成功する"))
            }
        });
        let buf_size = (frames * channels) as usize * 2;
        let mut buf = vec![0u8; buf_size];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            frames,
            channels,
            sample_rate,
            0,
        );
        assert_eq!(ret, frames);
        for (i, sample) in samples.iter().enumerate() {
            let offset = i * 2;
            let written = i16::from_le_bytes([buf[offset], buf[offset + 1]]);
            assert_eq!(written, *sample, "サンプル {i} が一致しない");
        }
    }

    #[test]
    fn same_format_s16_pads_with_silence_when_data_is_shorter() {
        let channels = 2;
        let partial_frames = 4;
        let buf_frames = 10;
        let sample_rate = 48000;
        let samples: Vec<i16> = (0..(partial_frames * channels) as i16).collect();
        let ctx = make_context({
            let samples = samples.clone();
            move |_, c, sr| {
                Some(PlaybackFrame::from_s16(&samples, c, sr).expect("S16 フレーム作成に成功する"))
            }
        });
        let buf_size = (buf_frames * channels) as usize * 2;
        let mut buf = vec![0xCCu8; buf_size];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            buf_frames,
            channels,
            sample_rate,
            0,
        );
        // 書き込まれるのは partial_frames 分
        assert_eq!(ret, partial_frames);
        // 書き込まれた部分の検証
        for (i, sample) in samples.iter().enumerate() {
            let offset = i * 2;
            let written = i16::from_le_bytes([buf[offset], buf[offset + 1]]);
            assert_eq!(written, *sample, "サンプル {i} が一致しない");
        }
        // 残りがゼロで埋められていることを確認する
        let written_bytes = (partial_frames * channels) as usize * 2;
        assert!(buf[written_bytes..].iter().all(|&b| b == 0));
    }

    // -----------------------------------------------------------------------
    // 同一フォーマット (F32 → F32)
    // -----------------------------------------------------------------------

    #[test]
    fn same_format_f32_copies_samples() {
        let channels = 1;
        let frames = 8;
        let sample_rate = 48000;
        let samples: Vec<f32> = vec![0.0, 0.25, -0.5, 0.75, -1.0, 0.125, 0.0, 0.5];
        let ctx = make_context({
            let samples = samples.clone();
            move |_, c, sr| {
                Some(PlaybackFrame::from_f32(&samples, c, sr).expect("F32 フレーム作成に成功する"))
            }
        });
        let buf_size = (frames * channels) as usize * 4;
        let mut buf = vec![0u8; buf_size];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            frames,
            channels,
            sample_rate,
            1,
        );
        assert_eq!(ret, frames);
        for (i, sample) in samples.iter().enumerate() {
            let offset = i * 4;
            let bytes: [u8; 4] = [
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ];
            let written = f32::from_le_bytes(bytes);
            assert!(
                (written - sample).abs() < 1e-7,
                "サンプル {i}: 期待 {sample}, 実際 {written}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // フォーマット変換 (F32 → S16)
    // -----------------------------------------------------------------------

    #[test]
    fn converts_f32_to_s16() {
        let channels = 1;
        let frames = 8;
        let sample_rate = 48000;
        let samples: Vec<f32> = vec![0.0, 0.5, -0.5, 1.0, -1.0, 0.25, 0.0, 0.0];
        let ctx = make_context({
            let samples = samples.clone();
            move |_, c, sr| {
                Some(PlaybackFrame::from_f32(&samples, c, sr).expect("F32 フレーム作成に成功する"))
            }
        });
        let buf_size = (frames * channels) as usize * 2;
        let mut buf = vec![0u8; buf_size];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            frames,
            channels,
            sample_rate,
            0,
        );
        assert_eq!(ret, frames);
        let expected: Vec<i16> = samples
            .iter()
            .map(|&s| {
                let clamped = if s.is_nan() { 0.0 } else { s.clamp(-1.0, 1.0) };
                (clamped * 32767.0) as i16
            })
            .collect();
        for (i, exp) in expected.iter().enumerate() {
            let offset = i * 2;
            let written = i16::from_le_bytes([buf[offset], buf[offset + 1]]);
            assert_eq!(written, *exp, "サンプル {i}: 期待 {exp}, 実際 {written}");
        }
    }

    #[test]
    fn f32_to_s16_handles_nan() {
        let channels = 1;
        let frames = 4;
        let sample_rate = 48000;
        let nan_data: Vec<f32> = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0];
        let ctx = make_context(move |_, c, sr| {
            Some(PlaybackFrame::from_f32(&nan_data, c, sr).expect("F32 フレーム作成に成功する"))
        });
        let buf_size = (frames * channels) as usize * 2;
        let mut buf = vec![0xFFu8; buf_size];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            frames,
            channels,
            sample_rate,
            0,
        );
        assert_eq!(ret, frames);
        // NaN → 0, +Inf → clamp → 32767, -Inf → clamp → -32767, 0.0 → 0
        let expected: [i16; 4] = [0, 32767, -32767, 0];
        for (i, exp) in expected.iter().enumerate() {
            let offset = i * 2;
            let written = i16::from_le_bytes([buf[offset], buf[offset + 1]]);
            assert_eq!(written, *exp, "サンプル {i}: 期待 {exp}, 実際 {written}");
        }
    }

    // -----------------------------------------------------------------------
    // フォーマット変換 (S16 → F32)
    // -----------------------------------------------------------------------

    #[test]
    fn converts_s16_to_f32() {
        let channels = 1;
        let frames = 8;
        let sample_rate = 48000;
        let samples: Vec<i16> = vec![0, 16384, -16384, 32767, -32767, 8192, 0, 0];
        let ctx = make_context({
            let samples = samples.clone();
            move |_, c, sr| {
                Some(PlaybackFrame::from_s16(&samples, c, sr).expect("S16 フレーム作成に成功する"))
            }
        });
        let buf_size = (frames * channels) as usize * 4;
        let mut buf = vec![0u8; buf_size];
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            frames,
            channels,
            sample_rate,
            1,
        );
        assert_eq!(ret, frames);
        let expected: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();
        for (i, exp) in expected.iter().enumerate() {
            let offset = i * 4;
            let bytes: [u8; 4] = [
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ];
            let written = f32::from_le_bytes(bytes);
            assert!(
                (written - exp).abs() < 1e-7,
                "サンプル {i}: 期待 {exp}, 実際 {written}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // バッファオーバーフローテスト
    // -----------------------------------------------------------------------

    #[test]
    fn returns_zero_when_buffer_size_overflows() {
        let ctx = make_context(|_, _, _| None);
        let mut buf = vec![0u8; 480];
        // frames * channels * bytes_per_sample が usize を超える値を与える
        let ret = playback_callback(
            context_ptr(&ctx),
            buf.as_mut_ptr() as *mut c_void,
            i32::MAX,
            i32::MAX,
            48000,
            0,
        );
        assert_eq!(ret, 0);
    }
}
