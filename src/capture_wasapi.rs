//! Windows 用オーディオキャプチャ (WASAPI)

use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use windows::{
    Win32::Foundation::*, Win32::Media::Audio::*, Win32::System::Com::*,
    Win32::System::Performance::*, Win32::System::Threading::*,
};

use crate::common::{AudioCaptureConfig, AudioDeviceType, AudioFormat, AudioFrame};
use crate::device_wasapi::{ComGuard, SendHandle, SendPtr, get_device_by_id};
use crate::error::{Error, Result};

pub(crate) struct WasapiCaptureContext {
    pub(crate) callback: Box<dyn Fn(AudioFrame<'_>) + Send + Sync>,
    pub(crate) running: AtomicBool,
}

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

pub(crate) struct WasapiCaptureImpl {
    session: Option<SessionData>,
    context: Option<Arc<WasapiCaptureContext>>,
    capture_thread: Option<thread::JoinHandle<()>>,
    config: AudioCaptureConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
}

impl WasapiCaptureImpl {
    pub fn new<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + Sync + 'static,
    {
        // デバイスを取得（キャプチャなので入力デバイス）
        let device = get_device_by_id(config.device_id.as_deref(), AudioDeviceType::Input)?;

        unsafe {
            // オーディオクライアントを取得
            let audio_client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|_| Error::DeviceAccessDenied)?;

            // ミックスフォーマットを取得
            let mix_format = audio_client
                .GetMixFormat()
                .map_err(|_| Error::DeviceAccessDenied)?;
            let wave_format = &*mix_format;

            // フォーマット情報を取得
            let format = crate::device_wasapi::determine_audio_format(mix_format);

            let sample_rate = wave_format.nSamplesPerSec as i32;
            let channels = wave_format.nChannels as i32;

            // オーディオクライアントを初期化（10ms バッファ）
            let buffer_duration: i64 = 100_000; // 100 ナノ秒単位で 10ms を指定する
            let init_result = audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                buffer_duration,
                0,
                mix_format,
                Some(std::ptr::null()),
            );

            // Initialize の成否にかかわらず mix_format を解放する
            CoTaskMemFree(Some(mix_format as *const _));

            init_result.map_err(|_| Error::SessionCreateFailed)?;

            // キャプチャクライアントを取得
            let capture_client: IAudioCaptureClient = audio_client
                .GetService()
                .map_err(|_| Error::SessionCreateFailed)?;

            // イベントハンドルを作成
            let event_handle =
                CreateEventW(None, false, false, None).map_err(|_| Error::SessionCreateFailed)?;

            // イベントハンドルを設定
            audio_client.SetEventHandle(event_handle).map_err(|_| {
                let _ = CloseHandle(event_handle);
                Error::SessionCreateFailed
            })?;

            let context = Arc::new(WasapiCaptureContext {
                callback: Box::new(callback),
                running: AtomicBool::new(false),
            });

            Ok(Self {
                session: Some(SessionData {
                    audio_client,
                    capture_client,
                    event_handle,
                    format,
                    sample_rate,
                    channels,
                }),
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

        // スレッド生成前に running フラグを立てる
        context.running.store(true, Ordering::Release);

        // キャプチャに必要なデータをクローン（Send ラッパーで包む）
        let capture_client = SendPtr(session.capture_client.clone());
        let event_handle = SendHandle(session.event_handle);
        let format = session.format;
        let sample_rate = session.sample_rate;
        let channels = session.channels;

        let thread_context = Arc::clone(context);

        // キャプチャスレッドを開始
        let handle = thread::Builder::new()
            .name("audio-capture".to_string())
            .spawn(move || {
                let _com_guard = ComGuard::init_mta();
                capture_thread_func(
                    capture_client.into_inner(),
                    event_handle.into_inner(),
                    format,
                    sample_rate,
                    channels,
                    thread_context,
                );
            })
            .map_err(|_| {
                context.running.store(false, Ordering::Release);
                unsafe {
                    let _ = session.audio_client.Stop();
                }
                Error::SessionStartFailed
            })?;

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

impl Drop for WasapiCaptureImpl {
    fn drop(&mut self) {
        self.stop();
        if let Some(session) = self.session.take() {
            unsafe {
                let _ = CloseHandle(session.event_handle);
            }
        }
    }
}

// Safety: COM オブジェクトは MTA (COINIT_MULTITHREADED) で初期化しており、
// MTA オブジェクトはスレッド間で安全に移送できる。
unsafe impl Send for WasapiCaptureImpl {}
unsafe impl Sync for WasapiCaptureImpl {}

/// キャプチャスレッド関数
fn capture_thread_func(
    capture_client: IAudioCaptureClient,
    event_handle: HANDLE,
    format: AudioFormat,
    sample_rate: i32,
    channels: i32,
    context: Arc<WasapiCaptureContext>,
) {
    unsafe {
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
                    let timestamp_us = qpc_counter_to_micros(counter, frequency);

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
                    (context.callback)(frame);
                }
                let _ = capture_client.ReleaseBuffer(frames_available);
            }
        }
    }
}

/// QueryPerformanceCounter が返すカウンター値をマイクロ秒へ変換する。
///
/// `frequency` が 0 以下の場合は無効な値とみなして 0 を返す。
fn qpc_counter_to_micros(counter: i64, frequency: i64) -> i64 {
    if frequency <= 0 {
        return 0;
    }

    // counter * 1_000_000 は QPC カウンターが大きくなると i64 の範囲を超えるため、
    // 中間計算は i128 で行う。counter / frequency はシステム起動からの経過時間 (秒) であり、
    // マイクロ秒に換算した値は i64 の範囲 (約 29 万年) に収まるため、i64 へのキャストは安全である。
    ((counter as i128) * 1_000_000 / (frequency as i128)) as i64
}

#[cfg(test)]
mod tests {
    use super::qpc_counter_to_micros;

    /// counter * 1_000_000 が i64 の最大値を超える境界値でも正しく変換できること
    #[test]
    fn converts_counter_whose_product_exceeds_i64_max() {
        // 10 MHz の QPC で counter * 1_000_000 が i64::MAX を超える最小の counter を渡す
        let frequency = 10_000_000;
        let counter = i64::MAX / 1_000_000 + 1;

        assert_eq!(
            qpc_counter_to_micros(counter, frequency),
            922_337_203_685,
            "i64 の乗算でオーバーフローする境界値でもパニックせず正しいマイクロ秒を返すこと"
        );
    }

    /// counter = frequency のときに 1 秒分のマイクロ秒を返すこと
    #[test]
    fn converts_one_second_to_one_million_micros() {
        assert_eq!(
            qpc_counter_to_micros(10_000_000, 10_000_000),
            1_000_000,
            "10 MHz のカウンターが 10,000,000 進むと 1 秒になること"
        );
    }

    /// frequency が 0 以下の場合に 0 を返すこと
    #[test]
    fn returns_zero_when_frequency_is_not_positive() {
        assert_eq!(
            qpc_counter_to_micros(1_000_000, 0),
            0,
            "frequency が 0 のときは 0 を返すこと"
        );
        assert_eq!(
            qpc_counter_to_micros(1_000_000, -1),
            0,
            "frequency が負のときは 0 を返すこと"
        );
    }
}
