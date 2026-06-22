//! Windows 用オーディオ再生 (WASAPI)

use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use windows::{
    Win32::Foundation::*, Win32::Media::Audio::*, Win32::System::Com::*,
    Win32::System::Threading::*,
};

use crate::common::{AudioDeviceType, AudioFormat, AudioPlaybackConfig, PlaybackFrame};
use crate::device_windows::{SendHandle, SendPtr, get_device_by_id};
use crate::error::{Error, Result};

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

// Safety: IAudioRenderClient / IAudioClient は COM の MTA (COINIT_MULTITHREADED) で初期化しており、
// MTA オブジェクトはスレッド間で安全に移送できる。
unsafe impl Send for SendPtr<IAudioRenderClient> {}
unsafe impl Send for SendPtr<IAudioClient> {}

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
        crate::device_windows::init_com_mta()?;
        unsafe {
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
            let format = crate::device_windows::determine_audio_format(mix_format);

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
        let handle = thread::Builder::new()
            .name("audio-playback".into())
            .spawn(move || {
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
            })
            .map_err(|_| Error::SessionStartFailed)?;

        // スレッド生成成功後に running フラグを立てる
        context.running.store(true, Ordering::Release);
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
        // ワーカースレッドの COM 参照カウント追加。
        // MTA はプロセス全体で共有されるため、呼び出し元で検証済みなら失敗しない。
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
            let frame_opt =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (context.callback)()))
                    .unwrap_or(None);

            // バッファを取得
            let data_ptr = match render_client.GetBuffer(frames_available) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if let Some(frame) = frame_opt {
                // フレームデータをバッファにコピー
                let bytes_per_sample: usize = match format {
                    AudioFormat::S16 => 2,
                    AudioFormat::F32 => 4,
                };
                let buffer_size = match (frames_available as usize)
                    .checked_mul(channels as usize)
                    .and_then(|n| n.checked_mul(bytes_per_sample))
                {
                    Some(size) => size,
                    None => {
                        let _ = render_client.ReleaseBuffer(frames_available, 0);
                        continue;
                    }
                };

                // フォーマット変換が必要な場合の処理
                if frame.format == AudioFormat::F32 && format == AudioFormat::S16 {
                    // F32 -> S16 変換
                    // Vec<u8> のアライメントは 1 なので read_unaligned で読み取る
                    let src_count = frame.data.len() / 4;
                    let dst_s16 =
                        std::slice::from_raw_parts_mut(data_ptr as *mut i16, buffer_size / 2);
                    let copy_len = src_count.min(dst_s16.len());
                    let src_ptr = frame.data.as_ptr() as *const f32;
                    for (i, dst) in dst_s16.iter_mut().enumerate().take(copy_len) {
                        let sample = src_ptr.add(i).read_unaligned();
                        *dst = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                    }
                    // 残りを無音で埋める
                    dst_s16[copy_len..].fill(0);
                } else if frame.format == AudioFormat::S16 && format == AudioFormat::F32 {
                    // S16 -> F32 変換
                    // Vec<u8> のアライメントは 1 なので read_unaligned で読み取る
                    let src_count = frame.data.len() / 2;
                    let dst_f32 =
                        std::slice::from_raw_parts_mut(data_ptr as *mut f32, buffer_size / 4);
                    let copy_len = src_count.min(dst_f32.len());
                    let src_ptr = frame.data.as_ptr() as *const i16;
                    for (i, dst) in dst_f32.iter_mut().enumerate().take(copy_len) {
                        let sample = src_ptr.add(i).read_unaligned();
                        *dst = sample as f32 / 32768.0;
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
