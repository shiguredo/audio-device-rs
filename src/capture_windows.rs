//! Windows 用オーディオキャプチャ (WASAPI)

use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use windows::{
    Win32::Foundation::*, Win32::Media::Audio::*, Win32::System::Com::*,
    Win32::System::Performance::*,
    Win32::System::Threading::*,
};

use crate::common::{AudioDeviceType, AudioFormat, AudioFrame, AudioCaptureConfig, CaptureContext};
use crate::device_windows::{get_device_by_id, SendHandle, SendPtr};
use crate::error::{Error, Result};

struct SessionData {
    audio_client: IAudioClient,
    capture_client: IAudioCaptureClient,
    event_handle: HANDLE,
    format: AudioFormat,
    sample_rate: i32,
    channels: i32,
}

// Safety: IAudioCaptureClient は COM の MTA (COINIT_MULTITHREADED) で初期化しており、
// MTA オブジェクトはスレッド間で安全に移送できる。
unsafe impl Send for SendPtr<IAudioCaptureClient> {}

pub struct AudioCapture {
    session: Option<SessionData>,
    context: Option<Arc<CaptureContext>>,
    capture_thread: Option<thread::JoinHandle<()>>,
    config: AudioCaptureConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
}

impl AudioCapture {
    pub fn new<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        crate::device_windows::init_com_mta()?;
        unsafe {
            // デバイスを取得（キャプチャなので入力デバイス）
            let device = get_device_by_id(config.device_id.as_deref(), AudioDeviceType::Input)?;

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

            // キャプチャクライアントを取得
            let capture_client: IAudioCaptureClient = audio_client.GetService().map_err(|_| {
                let _ = CloseHandle(event_handle);
                Error::SessionCreateFailed
            })?;

            let context = Arc::new(CaptureContext {
                callback: Box::new(callback),
                running: AtomicBool::new(false),
            });

            let session = SessionData {
                audio_client,
                capture_client,
                event_handle,
                format,
                sample_rate,
                channels,
            };

            Ok(Self {
                session: Some(session),
                context: Some(context),
                capture_thread: None,
                config,
                actual_sample_rate: sample_rate,
                actual_channels: channels,
            })
        }
    }

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

        // キャプチャに必要なデータをクローン（Send ラッパーで包む）
        let capture_client = SendPtr(session.capture_client.clone());
        let event_handle = SendHandle(session.event_handle);
        let format = session.format;
        let sample_rate = session.sample_rate;
        let channels = session.channels;
        let context_clone = Arc::clone(context);

        // キャプチャスレッドを開始
        let handle = thread::Builder::new()
            .name("audio-capture".into())
            .spawn(move || {
                capture_thread_func(
                    capture_client.into_inner(),
                    event_handle.into_inner(),
                    format,
                    sample_rate,
                    channels,
                    context_clone,
                );
            })
            .map_err(|_| Error::SessionStartFailed)?;

        // スレッド生成成功後に running フラグを立てる
        context.running.store(true, Ordering::Release);
        self.capture_thread = Some(handle);

        Ok(())
    }

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
            if let Some(handle) = self.capture_thread.take() {
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
            unsafe {
                let _ = CloseHandle(session.event_handle);
            }
        }
    }
}

unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

/// キャプチャスレッド関数
fn capture_thread_func(
    capture_client: IAudioCaptureClient,
    event_handle: HANDLE,
    format: AudioFormat,
    sample_rate: i32,
    channels: i32,
    context: Arc<CaptureContext>,
) {
    unsafe {
        // ワーカースレッドの COM 参照カウント追加。
        // MTA はプロセス全体で共有されるため、呼び出し元で検証済みなら失敗しない。
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        // タイムスタンプ用のパフォーマンスカウンタ
        let mut frequency = 0i64;
        let _ = QueryPerformanceFrequency(&mut frequency);

        while context.running.load(Ordering::Acquire) {
            // イベント待機（10ms タイムアウト）
            let wait_result = WaitForSingleObject(event_handle, 10);

            if !context.running.load(Ordering::Acquire) {
                break;
            }

            if wait_result != WAIT_OBJECT_0 && wait_result != WAIT_TIMEOUT {
                continue;
            }

            // 利用可能なパケットを処理
            loop {
                let packet_length = match capture_client.GetNextPacketSize() {
                    Ok(len) => len,
                    Err(_) => break,
                };

                if packet_length == 0 {
                    break;
                }

                if !context.running.load(Ordering::Acquire) {
                    break;
                }

                let mut data_ptr: *mut u8 = ptr::null_mut();
                let mut frames_available: u32 = 0;
                let mut flags: u32 = 0;

                if capture_client
                    .GetBuffer(&mut data_ptr, &mut frames_available, &mut flags, None, None)
                    .is_err()
                {
                    break;
                }

                if frames_available > 0 {
                    // タイムスタンプを取得（マイクロ秒）
                    let mut counter = 0i64;
                    let _ = QueryPerformanceCounter(&mut counter);
                    let timestamp_us = if frequency > 0 {
                        (counter * 1_000_000) / frequency
                    } else {
                        0
                    };

                    // データサイズを計算
                    let bytes_per_sample: usize = match format {
                        AudioFormat::S16 => 2,
                        AudioFormat::F32 => 4,
                    };
                    let data_size = match (frames_available as usize)
                        .checked_mul(channels as usize)
                        .and_then(|n| n.checked_mul(bytes_per_sample))
                    {
                        Some(size) => size,
                        None => {
                            let _ = capture_client.ReleaseBuffer(frames_available);
                            continue;
                        }
                    };

                    // 無音フラグまたは null ポインタの場合はスキップする
                    let is_silent = (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
                    if is_silent || data_ptr.is_null() || data_size == 0 {
                        let _ = capture_client.ReleaseBuffer(frames_available);
                        continue;
                    }

                    let data = std::slice::from_raw_parts(data_ptr, data_size);

                    let frame = AudioFrame {
                        data,
                        frames: frames_available as i32,
                        channels,
                        sample_rate,
                        format,
                        timestamp_us,
                    };

                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (context.callback)(frame);
                    }));
                }

                let _ = capture_client.ReleaseBuffer(frames_available);
            }
        }

        CoUninitialize();
    }
}
