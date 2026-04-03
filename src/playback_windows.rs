//! Windows 用オーディオ再生 (WASAPI)

use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use windows::{
    Win32::Foundation::*, Win32::Media::Audio::*, Win32::Media::KernelStreaming::*,
    Win32::Media::Multimedia::*, Win32::System::Com::*, Win32::System::Threading::*,
};

use crate::device_windows::{AudioDeviceType, AudioFormat, get_device_by_id};
use crate::error::{Error, Result};

/// 再生用オーディオフレームデータ
pub struct PlaybackFrame {
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
}

impl PlaybackFrame {
    /// S16 データから PlaybackFrame を作成
    pub fn from_s16(data: &[i16], channels: i32, sample_rate: i32) -> Result<Self> {
        if channels <= 0 {
            return Err(Error::InvalidChannels);
        }
        let frames = data.len() as i32 / channels;
        let bytes: Vec<u8> = data
            .iter()
            .flat_map(|&sample| sample.to_le_bytes())
            .collect();
        Ok(Self {
            data: bytes,
            frames,
            channels,
            sample_rate,
            format: AudioFormat::S16,
        })
    }

    /// F32 データから PlaybackFrame を作成
    pub fn from_f32(data: &[f32], channels: i32, sample_rate: i32) -> Result<Self> {
        if channels <= 0 {
            return Err(Error::InvalidChannels);
        }
        let frames = data.len() as i32 / channels;
        let bytes: Vec<u8> = data
            .iter()
            .flat_map(|&sample| sample.to_le_bytes())
            .collect();
        Ok(Self {
            data: bytes,
            frames,
            channels,
            sample_rate,
            format: AudioFormat::F32,
        })
    }
}

/// オーディオ再生設定
pub struct AudioPlaybackConfig {
    /// デバイス ID（None の場合はデフォルトデバイス）
    pub device_id: Option<String>,
    /// サンプルレート（0 の場合はデバイスのデフォルト）
    pub sample_rate: i32,
    /// チャンネル数（0 の場合はデバイスのデフォルト）
    pub channels: i32,
}

impl Default for AudioPlaybackConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            sample_rate: 48000,
            channels: 2,
        }
    }
}

struct PlaybackContext {
    callback: Box<dyn Fn() -> Option<PlaybackFrame> + Send + Sync>,
    running: AtomicBool,
}

struct SessionData {
    audio_client: IAudioClient,
    render_client: IAudioRenderClient,
    event_handle: HANDLE,
    format: AudioFormat,
    sample_rate: i32,
    channels: i32,
    buffer_frames: u32,
}

/// Send でない型をスレッドに渡すためのラッパー（MTA で初期化済みのため安全）
struct SendHandle(HANDLE);
unsafe impl Send for SendHandle {}
impl SendHandle {
    fn into_inner(self) -> HANDLE {
        self.0
    }
}

struct SendPtr<T>(T);
unsafe impl<T> Send for SendPtr<T> {}
impl<T> SendPtr<T> {
    fn into_inner(self) -> T {
        self.0
    }
}

/// オーディオ再生
pub struct AudioPlayback {
    session: Option<SessionData>,
    context: Option<Arc<PlaybackContext>>,
    playback_thread: Option<thread::JoinHandle<()>>,
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
        unsafe {
            // COM 初期化 (既に初期化済みの場合も許容する)
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            // デバイスを取得（再生なので出力デバイス）
            let device = get_device_by_id(config.device_id.as_deref(), AudioDeviceType::Output)?;

            // オーディオクライアントを取得
            let audio_client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|_| Error::SessionCreateFailed)?;

            // ミックスフォーマットを取得
            let mix_format = audio_client
                .GetMixFormat()
                .map_err(|_| Error::SessionCreateFailed)?;

            // フォーマット情報を取得
            let sample_rate = (*mix_format).nSamplesPerSec as i32;
            let channels = (*mix_format).nChannels as i32;
            let format = determine_playback_format(mix_format);

            // イベントハンドルを作成
            let event_handle =
                CreateEventW(None, false, false, None).map_err(|_| Error::SessionCreateFailed)?;

            // オーディオクライアントを初期化（10ms バッファ）
            let buffer_duration: i64 = 100_000; // 10ms in 100-nanosecond units
            audio_client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    buffer_duration,
                    0,
                    mix_format,
                    None,
                )
                .map_err(|_| {
                    CoTaskMemFree(Some(mix_format as *const _));
                    let _ = CloseHandle(event_handle);
                    Error::SessionCreateFailed
                })?;

            CoTaskMemFree(Some(mix_format as *const _));

            // イベントハンドルを設定
            audio_client.SetEventHandle(event_handle).map_err(|_| {
                let _ = CloseHandle(event_handle);
                Error::SessionCreateFailed
            })?;

            // バッファサイズを取得
            let buffer_frames = audio_client.GetBufferSize().map_err(|_| {
                let _ = CloseHandle(event_handle);
                Error::SessionCreateFailed
            })?;

            // レンダークライアントを取得
            let render_client: IAudioRenderClient = audio_client.GetService().map_err(|_| {
                let _ = CloseHandle(event_handle);
                Error::SessionCreateFailed
            })?;

            let context = Arc::new(PlaybackContext {
                callback: Box::new(callback),
                running: AtomicBool::new(false),
            });

            let session = SessionData {
                audio_client,
                render_client,
                event_handle,
                format,
                sample_rate,
                channels,
                buffer_frames,
            };

            Ok(Self {
                session: Some(session),
                context: Some(context),
                playback_thread: None,
                config,
                actual_sample_rate: sample_rate,
                actual_channels: channels,
            })
        }
    }

    /// 再生を開始
    pub fn start(&mut self) -> Result<()> {
        let session = self.session.as_ref().ok_or(Error::SessionStartFailed)?;
        let context = self.context.as_ref().ok_or(Error::SessionStartFailed)?;

        if context.running.load(Ordering::Acquire) {
            return Ok(());
        }

        // オーディオクライアントを開始
        unsafe {
            session
                .audio_client
                .Start()
                .map_err(|_| Error::SessionStartFailed)?;
        }

        context.running.store(true, Ordering::Release);

        // 再生に必要なデータをクローン（Send ラッパーで包む）
        let render_client = SendPtr(session.render_client.clone());
        let audio_client = SendPtr(session.audio_client.clone());
        let event_handle = SendHandle(session.event_handle);
        let format = session.format;
        let sample_rate = session.sample_rate;
        let channels = session.channels;
        let buffer_frames = session.buffer_frames;
        let context_clone = Arc::clone(context);

        // 再生スレッドを開始
        let handle = thread::spawn(move || {
            playback_thread_func(
                render_client.into_inner(),
                audio_client.into_inner(),
                event_handle.into_inner(),
                format,
                sample_rate,
                channels,
                buffer_frames,
                context_clone,
            );
        });

        self.playback_thread = Some(handle);

        Ok(())
    }

    /// 再生を停止
    pub fn stop(&mut self) {
        if let Some(context) = &self.context
            && context.running.load(Ordering::Acquire)
        {
            context.running.store(false, Ordering::Release);

            // イベントをシグナル状態にしてスレッドを起こす
            if let Some(session) = &self.session {
                unsafe {
                    let _ = SetEvent(session.event_handle);
                }
            }

            // スレッドの終了を待機
            if let Some(handle) = self.playback_thread.take() {
                let _ = handle.join();
            }

            // オーディオクライアントを停止
            if let Some(session) = &self.session {
                unsafe {
                    let _ = session.audio_client.Stop();
                }
            }
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
            unsafe {
                let _ = CloseHandle(session.event_handle);
            }
        }
    }
}

unsafe impl Send for AudioPlayback {}
unsafe impl Sync for AudioPlayback {}

/// オーディオフォーマットを判定
unsafe fn determine_playback_format(wave_format: *const WAVEFORMATEX) -> AudioFormat {
    let format_tag = unsafe { (*wave_format).wFormatTag };

    if format_tag == WAVE_FORMAT_IEEE_FLOAT as u16 {
        return AudioFormat::F32;
    }

    if format_tag == WAVE_FORMAT_EXTENSIBLE as u16 {
        let ext = wave_format as *const WAVEFORMATEXTENSIBLE;
        let sub_format = unsafe { std::ptr::addr_of!((*ext).SubFormat).read_unaligned() };
        if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
            return AudioFormat::F32;
        }
    }

    AudioFormat::S16
}

/// 再生スレッド関数
#[allow(clippy::too_many_arguments)]
fn playback_thread_func(
    render_client: IAudioRenderClient,
    audio_client: IAudioClient,
    event_handle: HANDLE,
    format: AudioFormat,
    _sample_rate: i32,
    channels: i32,
    buffer_frames: u32,
    context: Arc<PlaybackContext>,
) {
    unsafe {
        // スレッドでも COM 初期化
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        while context.running.load(Ordering::Acquire) {
            // イベント待機（10ms タイムアウト）
            let wait_result = WaitForSingleObject(event_handle, 10);

            if !context.running.load(Ordering::Acquire) {
                break;
            }

            if wait_result != WAIT_OBJECT_0 && wait_result != WAIT_TIMEOUT {
                continue;
            }

            // バッファの空き状況を確認
            let padding = match audio_client.GetCurrentPadding() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let frames_available = buffer_frames.saturating_sub(padding);
            if frames_available == 0 {
                continue;
            }

            // コールバックからデータを取得
            let frame_opt = (context.callback)();

            // バッファを取得
            let data_ptr = match render_client.GetBuffer(frames_available) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if let Some(frame) = frame_opt {
                // フレームデータをバッファにコピー
                let bytes_per_sample = match format {
                    AudioFormat::S16 => 2,
                    AudioFormat::F32 => 4,
                };
                let buffer_size = (frames_available as i32 * channels * bytes_per_sample) as usize;

                // フォーマット変換が必要な場合の処理
                if frame.format == AudioFormat::F32 && format == AudioFormat::S16 {
                    // F32 -> S16 変換
                    let src_f32 = std::slice::from_raw_parts(
                        frame.data.as_ptr() as *const f32,
                        frame.data.len() / 4,
                    );
                    let dst_s16 =
                        std::slice::from_raw_parts_mut(data_ptr as *mut i16, buffer_size / 2);
                    let copy_len = src_f32.len().min(dst_s16.len());
                    for i in 0..copy_len {
                        dst_s16[i] = (src_f32[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
                    }
                    // 残りを無音で埋める
                    dst_s16[copy_len..].fill(0);
                } else if frame.format == AudioFormat::S16 && format == AudioFormat::F32 {
                    // S16 -> F32 変換
                    let src_s16 = std::slice::from_raw_parts(
                        frame.data.as_ptr() as *const i16,
                        frame.data.len() / 2,
                    );
                    let dst_f32 =
                        std::slice::from_raw_parts_mut(data_ptr as *mut f32, buffer_size / 4);
                    let copy_len = src_s16.len().min(dst_f32.len());
                    for i in 0..copy_len {
                        dst_f32[i] = src_s16[i] as f32 / 32768.0;
                    }
                    // 残りを無音で埋める
                    dst_f32[copy_len..].fill(0.0);
                } else {
                    // 同じフォーマット、そのままコピー
                    let copy_len = frame.data.len().min(buffer_size);
                    ptr::copy_nonoverlapping(frame.data.as_ptr(), data_ptr, copy_len);
                    // 残りを無音で埋める
                    if copy_len < buffer_size {
                        ptr::write_bytes(data_ptr.add(copy_len), 0, buffer_size - copy_len);
                    }
                }

                let _ = render_client.ReleaseBuffer(frames_available, 0);
            } else {
                // データがない場合は無音
                let _ = render_client
                    .ReleaseBuffer(frames_available, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32);
            }
        }

        CoUninitialize();
    }
}
