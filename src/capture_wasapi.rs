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

            // 要求するフォーマットを構築する
            let wave_format = build_capture_wave_format(config.sample_rate, config.channels)?;
            let sample_rate = wave_format.nSamplesPerSec as i32;
            let channels = wave_format.nChannels as i32;
            let format = AudioFormat::S16;

            // オーディオクライアントを初期化する。共有モードの形式変換を有効にして、
            // デバイスの形式から要求フォーマットへ変換させる。イベント駆動の共有モードでは
            // バッファ長をオーディオエンジンが決めるため、hnsBufferDuration と
            // hnsPeriodicity は 0 にする。
            let init_result = audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                0,
                0,
                &wave_format,
                Some(ptr::null()),
            );
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

/// 指定されたサンプルレートとチャンネル数から、キャプチャで要求する S16 の
/// `WAVEFORMATEX` を構築する。
///
/// `sample_rate` が 0 以下の場合は 48000、`channels` が 0 以下の場合は 1 ch として扱う。
///
/// # エラー
///
/// - 1 サンプルフレームあたりのバイト数が `WAVEFORMATEX` の `u16` フィールドに収まらない場合
/// - 1 秒あたりのバイト数が `WAVEFORMATEX` の `u32` フィールドに収まらない場合
///
/// はいずれも [`Error::SessionCreateFailed`] を返す。
fn build_capture_wave_format(sample_rate: i32, channels: i32) -> Result<WAVEFORMATEX> {
    // 0 以下の値は既定値に丸める
    let sample_rate = if sample_rate <= 0 { 48000 } else { sample_rate };
    let channels = if channels <= 0 { 1 } else { channels };

    // S16 は 1 サンプルあたり 2 バイトなので、1 フレームあたりのバイト数は
    // チャンネル数の 2 倍になる。i32 の範囲の値でも乗算がオーバーフローしないよう
    // u64 で計算してから各フィールドの型に変換する。
    let channels = u16::try_from(channels).map_err(|_| Error::SessionCreateFailed)?;
    let block_align = (channels as u64) * 2;
    let block_align = u16::try_from(block_align).map_err(|_| Error::SessionCreateFailed)?;
    let avg_bytes_per_sec = (sample_rate as u64) * (block_align as u64);
    let avg_bytes_per_sec =
        u32::try_from(avg_bytes_per_sec).map_err(|_| Error::SessionCreateFailed)?;

    Ok(WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: sample_rate as u32,
        nAvgBytesPerSec: avg_bytes_per_sec,
        nBlockAlign: block_align,
        wBitsPerSample: 16,
        cbSize: 0,
    })
}

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
    use super::{build_capture_wave_format, qpc_counter_to_micros};
    use crate::error::Error;
    use windows::Win32::Media::Audio::{WAVE_FORMAT_PCM, WAVEFORMATEX};

    /// packed な `WAVEFORMATEX` のフィールドを値として取り出したもの
    struct WaveFormat {
        format_tag: u16,
        channels: u16,
        sample_rate: u32,
        avg_bytes_per_sec: u32,
        block_align: u16,
        bits_per_sample: u16,
        cb_size: u16,
    }

    /// `WAVEFORMATEX` の各フィールドを値として取り出す。
    fn unpack(format: WAVEFORMATEX) -> WaveFormat {
        // WAVEFORMATEX は packed なので、フィールドの参照を作らずに値をコピーする。
        WaveFormat {
            format_tag: format.wFormatTag,
            channels: format.nChannels,
            sample_rate: format.nSamplesPerSec,
            avg_bytes_per_sec: format.nAvgBytesPerSec,
            block_align: format.nBlockAlign,
            bits_per_sample: format.wBitsPerSample,
            cb_size: format.cbSize,
        }
    }

    /// 48000 Hz 1 ch から S16 の WAVEFORMATEX を構築すること
    #[test]
    fn builds_mono_s16_wave_format() {
        let format = unpack(
            build_capture_wave_format(48000, 1)
                .expect("48000 Hz 1 ch は WAVEFORMATEX に収まること"),
        );
        assert_eq!(
            format.format_tag, WAVE_FORMAT_PCM as u16,
            "PCM を指定すること"
        );
        assert_eq!(format.channels, 1, "チャンネル数を指定すること");
        assert_eq!(format.sample_rate, 48000, "サンプルレートを指定すること");
        assert_eq!(
            format.avg_bytes_per_sec, 96000,
            "1 秒あたり 48000 サンプル * 2 バイトになること"
        );
        assert_eq!(format.block_align, 2, "1 フレームあたり 2 バイトになること");
        assert_eq!(
            format.bits_per_sample, 16,
            "サンプルあたり 16 bit を指定すること"
        );
        assert_eq!(format.cb_size, 0, "拡張領域を持たないこと");
    }

    /// 44100 Hz 2 ch では 1 フレームと 1 秒あたりのバイト数が 2 倍になること
    #[test]
    fn builds_stereo_s16_wave_format() {
        let format = unpack(
            build_capture_wave_format(44100, 2)
                .expect("44100 Hz 2 ch は WAVEFORMATEX に収まること"),
        );
        assert_eq!(format.channels, 2, "チャンネル数を指定すること");
        assert_eq!(
            format.block_align, 4,
            "1 フレームあたり 2 ch * 2 バイトになること"
        );
        assert_eq!(
            format.avg_bytes_per_sec, 176400,
            "1 秒あたり 44100 サンプル * 4 バイトになること"
        );
    }

    /// 0 以下の値は 48000 Hz 1 ch として扱うこと
    #[test]
    fn clamps_non_positive_values() {
        for (sample_rate, channels) in [(0, 0), (-1, -1), (-48000, -2)] {
            let format = unpack(
                build_capture_wave_format(sample_rate, channels)
                    .expect("0 以下の値は既定値に丸められ WAVEFORMATEX に収まること"),
            );
            assert_eq!(
                format.sample_rate, 48000,
                "sample_rate={sample_rate} は 48000 Hz として扱うこと"
            );
            assert_eq!(
                format.channels, 1,
                "channels={channels} は 1 ch として扱うこと"
            );
            assert_eq!(format.block_align, 2, "1 フレームあたり 2 バイトになること");
            assert_eq!(
                format.avg_bytes_per_sec, 96000,
                "1 秒あたり 48000 サンプル * 2 バイトになること"
            );
        }

        // 片方だけが 0 以下の場合も、その項目だけを丸める
        let format =
            unpack(build_capture_wave_format(0, 2).expect("48000 Hz 2 ch として扱えること"));
        assert_eq!(format.sample_rate, 48000, "48000 Hz として扱うこと");
        assert_eq!(format.channels, 2, "指定したチャンネル数を維持すること");
    }

    /// 1 フレームあたりのバイト数が u16 に収まらないチャンネル数を拒否すること
    #[test]
    fn rejects_channels_whose_block_align_exceeds_u16() {
        // 32767 ch では 65534 バイトになり u16 の最大値に収まる
        let format = unpack(
            build_capture_wave_format(48000, 32767).expect("65534 バイトは u16 に収まること"),
        );
        assert_eq!(
            format.block_align,
            u16::MAX - 1,
            "32767 ch * 2 バイトが nBlockAlign に入ること"
        );

        // 32768 ch では 65536 バイトになり u16 の最大値を超える
        assert!(
            matches!(
                build_capture_wave_format(48000, 32768),
                Err(Error::SessionCreateFailed)
            ),
            "nBlockAlign が u16 に収まらない場合は SessionCreateFailed になること"
        );

        // 65536 ch は nChannels 自体が u16 に収まらない
        assert!(
            matches!(
                build_capture_wave_format(48000, 65536),
                Err(Error::SessionCreateFailed)
            ),
            "nChannels が u16 に収まらない場合は SessionCreateFailed になること"
        );
    }

    /// 1 秒あたりのバイト数が u32 に収まらない場合は拒否すること
    #[test]
    fn rejects_avg_bytes_per_sec_that_exceeds_u32() {
        // i32::MAX Hz 1 ch では 4294967294 バイト/秒になり u32 の最大値に収まる
        let format = unpack(
            build_capture_wave_format(i32::MAX, 1)
                .expect("4294967294 バイト/秒は u32 に収まること"),
        );
        assert_eq!(
            format.avg_bytes_per_sec,
            u32::MAX - 1,
            "i32::MAX Hz 1 ch * 2 バイトが nAvgBytesPerSec に入ること"
        );

        // i32::MAX Hz 2 ch では 8589934588 バイト/秒になり u32 の最大値を超える
        assert!(
            matches!(
                build_capture_wave_format(i32::MAX, 2),
                Err(Error::SessionCreateFailed)
            ),
            "nAvgBytesPerSec が u32 に収まらない場合は SessionCreateFailed になること"
        );
    }

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
