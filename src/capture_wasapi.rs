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
use crate::device_wasapi::{ComGuard, SendHandle, SendPtr, build_wave_format, get_device_by_id};
use crate::error::{Error, Result};

pub(crate) struct WasapiCaptureContext {
    pub(crate) callback: Box<dyn Fn(AudioFrame<'_>) + Send>,
}

// SAFETY: WasapiCaptureContext は WasapiCaptureImpl の Box に保持され、キャプチャ
// スレッドの実行中は drop されない。stop はスレッドを join してから戻るため、
// スレッドは join 後にコンテキストへアクセスしない。コールバックはキャプチャ
// スレッドからのみ直列に呼ばれ、その実行中に WasapiCaptureImpl 側が context へ
// アクセスすることもない。
unsafe impl Send for SendPtr<*mut WasapiCaptureContext> {}

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
    /// ユーザーコールバックを保持するコンテキスト
    context: Box<WasapiCaptureContext>,
    /// キャプチャスレッドと停止処理で共有する実行状態
    running: Arc<AtomicBool>,
    capture_thread: Option<thread::JoinHandle<()>>,
    config: AudioCaptureConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
}

impl WasapiCaptureImpl {
    pub fn new<F>(config: AudioCaptureConfig, callback: F) -> Result<Self>
    where
        F: Fn(AudioFrame<'_>) + Send + 'static,
    {
        // デバイスを取得（キャプチャなので入力デバイス）
        let device = get_device_by_id(config.device_id.as_deref(), AudioDeviceType::Input)?;

        unsafe {
            // オーディオクライアントを取得
            let audio_client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|_| Error::DeviceAccessDenied)?;

            // 要求するフォーマットを構築する。0 以下の値は既定値に丸める
            let sample_rate = if config.sample_rate <= 0 {
                48000
            } else {
                config.sample_rate as u32
            };
            let channels = if config.channels <= 0 {
                1
            } else {
                config.channels
            };
            let channels = u16::try_from(channels).map_err(|_| Error::InvalidAudioFormat)?;
            let wave_format = build_wave_format(AudioFormat::S16, sample_rate, channels)?;
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

            let context = Box::new(WasapiCaptureContext {
                callback: Box::new(callback),
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
                context,
                running: Arc::new(AtomicBool::new(false)),
                capture_thread: None,
                config,
                actual_sample_rate: sample_rate,
                actual_channels: channels,
            })
        }
    }

    pub fn start(&mut self) -> Result<()> {
        let session = self.session.as_ref().ok_or(Error::SessionStartFailed)?;

        // オーディオクライアントを開始
        unsafe {
            session
                .audio_client
                .Start()
                .map_err(|_| Error::SessionStartFailed)?;
        }

        // スレッド生成前に running フラグを立てる
        self.running.store(true, Ordering::Release);

        // キャプチャに必要なデータをクローン（Send ラッパーで包む）
        let capture_client = SendPtr(session.capture_client.clone());
        let event_handle = SendHandle(session.event_handle);
        let format = session.format;
        let sample_rate = session.sample_rate;
        let channels = session.channels;

        // コンテキストは self に残したまま、そのポインタだけをスレッドへ渡す。
        // Box の内容は WasapiCaptureImpl が drop されるまで移動しないため、
        // ポインタはスレッドの実行中ずっと有効である。
        let context = SendPtr(std::ptr::from_ref(&*self.context).cast_mut());
        let running = Arc::clone(&self.running);

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
                    context.into_inner(),
                    running,
                );
            })
            .map_err(|_| {
                self.running.store(false, Ordering::Release);
                unsafe {
                    let _ = session.audio_client.Stop();
                }
                Error::SessionStartFailed
            })?;

        self.capture_thread = Some(handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        if self.running.load(Ordering::Acquire) {
            self.running.store(false, Ordering::Release);

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

// SAFETY: COM オブジェクトは MTA (COINIT_MULTITHREADED) で初期化しており、
// MTA オブジェクトはスレッド間で安全に移送できる。
unsafe impl Send for WasapiCaptureImpl {}

/// パケットのキャプチャ時刻とデバイス位置の組。
///
/// `device_position` はストリーム開始からのサンプルフレーム数、`timestamp_us` は
/// そのパケット先頭のキャプチャ時刻（マイクロ秒）である。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimestampAnchor {
    device_position: u64,
    timestamp_us: i64,
}

/// キャプチャスレッド関数
///
/// `context` は `WasapiCaptureContext` を指す。この関数が戻るまで、その
/// コンテキストが存続することを前提とする。
fn capture_thread_func(
    capture_client: IAudioCaptureClient,
    event_handle: HANDLE,
    format: AudioFormat,
    sample_rate: i32,
    channels: i32,
    context: *mut WasapiCaptureContext,
    running: Arc<AtomicBool>,
) {
    // SAFETY: context は WasapiCaptureImpl の Box が保持する WasapiCaptureContext を
    // 指す。stop はこのスレッドを join してから戻るため、このスレッドの実行中は
    // コンテキストが生存する。コールバックはこのスレッドからしか呼ばれない。
    let context = unsafe { &*context };

    unsafe {
        // タイムスタンプ用のパフォーマンスカウンタ
        let mut frequency = 0i64;
        let _ = QueryPerformanceFrequency(&mut frequency);

        // タイムスタンプの補間に使う直前のパケットの基準
        let mut last_anchor: Option<TimestampAnchor> = None;
        // コールバックへ渡した直前のパケットのタイムスタンプ
        let mut last_timestamp_us: Option<i64> = None;

        while running.load(Ordering::Acquire) {
            // イベント待機（10ms タイムアウト）
            let wait_result = WaitForSingleObject(event_handle, 10);
            if !running.load(Ordering::Acquire) {
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
                if !running.load(Ordering::Acquire) {
                    break;
                }

                let mut data_ptr: *mut u8 = ptr::null_mut();
                let mut frames_available: u32 = 0;
                let mut flags: u32 = 0;
                let mut device_position: u64 = 0;
                let mut qpc_position: u64 = 0;
                if capture_client
                    .GetBuffer(
                        &mut data_ptr,
                        &mut frames_available,
                        &mut flags,
                        Some(&mut device_position),
                        Some(&mut qpc_position),
                    )
                    .is_err()
                {
                    break;
                }

                if frames_available > 0 {
                    // QPC 位置が使えない場合に備えて、読み出し時刻も取得しておく
                    let mut counter = 0i64;
                    let _ = QueryPerformanceCounter(&mut counter);
                    let read_time_us = qpc_counter_to_micros(counter, frequency);

                    let (timestamp_us, new_anchor) = timestamp_for_packet(
                        flags,
                        qpc_position,
                        device_position,
                        sample_rate,
                        read_time_us,
                        last_anchor,
                        last_timestamp_us,
                    );
                    last_anchor = new_anchor;

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

                    last_timestamp_us = Some(timestamp_us);

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

/// パケットのキャプチャ時刻を求める。
///
/// `flags` はバッファ状態フラグ、
/// `qpc_position` はパケット先頭フレームのデバイス位置を記録した時点の QPC 値を 100 ナノ秒単位に換算した値、
/// `device_position` はストリーム開始からのサンプルフレーム数、
/// `sample_rate` は 1 秒あたりのサンプルフレーム数、
/// `read_time_us` はマイクロ秒単位の読み出し時刻、
/// `last_anchor` は直前のパケットの基準、
/// `last_timestamp_us` は直前のパケットに付けてコールバックへ渡したタイムスタンプである。
///
/// 戻り値は `(タイムスタンプ, 補間の基準)` である。
/// タイムスタンプはマイクロ秒単位となり、`last_timestamp_us` 以下にならないようクランプされる。
/// 補間の基準は、`qpc_position` を使った場合や補間できた場合は `Some(TimestampAnchor)`、
/// 読み出し時刻へフォールバックした場合は `None` になる。基準の `timestamp_us` は
/// クランプ前の音声時刻である。
fn timestamp_for_packet(
    flags: u32,
    qpc_position: u64,
    device_position: u64,
    sample_rate: i32,
    read_time_us: i64,
    last_anchor: Option<TimestampAnchor>,
    last_timestamp_us: Option<i64>,
) -> (i64, Option<TimestampAnchor>) {
    // TIMESTAMP_ERROR はデバイス位置を記録した時刻が不確実であることを示し、
    // DATA_DISCONTINUITY はパケットが直前のパケットのデバイス位置と相関しないことを示す。
    let timestamp_error = (flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32) != 0;
    let data_discontinuity = (flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32) != 0;

    let (timestamp_us, new_anchor) = if !timestamp_error {
        // qpc_position が使える場合は、それをパケット先頭の時刻とする
        let timestamp_us = qpc_position_to_micros(qpc_position);
        (
            timestamp_us,
            Some(TimestampAnchor {
                device_position,
                timestamp_us,
            }),
        )
    } else if !data_discontinuity
        && let Some(anchor) = last_anchor
        // qpc_position が使えず、データが途切れていない場合は、直前の基準から補間する
        && let Some(timestamp_us) = interpolate_timestamp_us(
            anchor.timestamp_us,
            anchor.device_position,
            device_position,
            sample_rate,
        )
    {
        (
            timestamp_us,
            Some(TimestampAnchor {
                device_position,
                timestamp_us,
            }),
        )
    } else {
        // qpc_position の利用も補間もできなかった場合、読み出し時刻を利用する
        // 読み出し時刻は補間の基準にはしない
        (read_time_us, None)
    };

    // タイムスタンプが逆行してしまうのを防ぐためクランプする
    (
        clamp_timestamp_us(timestamp_us, last_timestamp_us),
        new_anchor,
    )
}

/// 出力するタイムスタンプが直前のパケットのタイムスタンプ以下にならないようにする。
///
/// `last_timestamp_us` が `None` の場合は `timestamp_us` をそのまま返す。
/// `timestamp_us` が `last_timestamp_us` 以下の場合は、`last_timestamp_us` に
/// 1 マイクロ秒を加えた値を返す。
/// `last_timestamp_us` が `i64::MAX` に達している場合は `i64::MAX` を返す。
fn clamp_timestamp_us(timestamp_us: i64, last_timestamp_us: Option<i64>) -> i64 {
    match last_timestamp_us {
        Some(last) if timestamp_us <= last => last.saturating_add(1),
        _ => timestamp_us,
    }
}

/// QPC 位置をマイクロ秒へ変換する。
fn qpc_position_to_micros(qpc_position: u64) -> i64 {
    // QPC 位置は 100 ナノ秒単位であるため、10 で割るとマイクロ秒になる
    (qpc_position / 10) as i64
}

/// 直前の基準から、デバイス位置の差に対応する時間を加算したタイムスタンプを求める。
///
/// `last_device_position` と `device_position` はサンプルフレーム単位のデバイス位置である。
/// `sample_rate` が 0 以下、デバイス位置が逆転している、計算結果が `i64` の範囲を超える
/// 場合は `None` を返す。
fn interpolate_timestamp_us(
    last_timestamp_us: i64,
    last_device_position: u64,
    device_position: u64,
    sample_rate: i32,
) -> Option<i64> {
    if sample_rate <= 0 || device_position < last_device_position {
        return None;
    }

    // フレーム数の差をマイクロ秒へ換算する。u64 の最大値に 1,000,000 を掛けても
    // i128 の範囲に収まるため、中間計算は i128 で行う
    let elapsed_frames = (device_position - last_device_position) as i128;
    let elapsed_us = elapsed_frames * 1_000_000 / (sample_rate as i128);
    i64::try_from((last_timestamp_us as i128) + elapsed_us).ok()
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
    use super::{
        TimestampAnchor, clamp_timestamp_us, interpolate_timestamp_us, qpc_counter_to_micros,
        qpc_position_to_micros, timestamp_for_packet,
    };
    use windows::Win32::Media::Audio::{
        AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
        AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR,
    };

    /// タイムスタンプが無効であることを示すフラグ
    const TIMESTAMP_ERROR: u32 = AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32;

    /// データの途切れを示すフラグ
    const DATA_DISCONTINUITY: u32 = AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32;

    /// 無音パケットを示すフラグ
    const SILENT: u32 = AUDCLNT_BUFFERFLAGS_SILENT.0 as u32;

    /// デバイス位置とタイムスタンプから基準を作る。
    fn anchor(device_position: u64, timestamp_us: i64) -> TimestampAnchor {
        TimestampAnchor {
            device_position,
            timestamp_us,
        }
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

    /// タイムスタンプエラーがなく QPC 位置が有効な場合は、QPC 位置のマイクロ秒を
    /// 使い、そのパケットを次の補間の基準にすること
    #[test]
    fn uses_qpc_position_as_timestamp() {
        let (timestamp_us, new_anchor) =
            timestamp_for_packet(0, 100_000_000, 480, 48000, 7_000_000, None, None);

        assert_eq!(
            timestamp_us, 10_000_000,
            "100 ナノ秒単位の QPC 位置をマイクロ秒に換算すること"
        );
        assert_eq!(
            new_anchor,
            Some(anchor(480, 10_000_000)),
            "QPC 位置を使ったパケットを基準にすること"
        );
    }

    /// 直前のタイムスタンプ以下になる場合は、直前 + 1 マイクロ秒にクランプし、
    /// 基準はクランプ前の音声時刻のままにすること
    #[test]
    fn clamps_timestamp_but_keeps_anchor_raw() {
        let (timestamp_us, new_anchor) = timestamp_for_packet(
            0,
            100_000_000,
            480,
            48000,
            7_000_000,
            None,
            Some(10_000_000),
        );

        assert_eq!(
            timestamp_us, 10_000_001,
            "直前のタイムスタンプ以下にならないこと"
        );
        assert_eq!(
            new_anchor,
            Some(anchor(480, 10_000_000)),
            "基準はクランプ前の音声時刻のままであること"
        );
    }

    /// 読み出し時刻へのフォールバックでも、直前のタイムスタンプ以下にならないこと
    #[test]
    fn clamps_read_time_fallback() {
        let (timestamp_us, new_anchor) = timestamp_for_packet(
            TIMESTAMP_ERROR,
            0,
            960,
            48000,
            7_000_000,
            None,
            Some(7_000_000),
        );

        assert_eq!(
            timestamp_us, 7_000_001,
            "読み出し時刻も直前のタイムスタンプ以下にならないこと"
        );
        assert_eq!(new_anchor, None, "読み出し時刻を補間の基準にしないこと");
    }

    /// 無音フラグだけが立っている場合は、QPC 位置をそのまま使うこと
    #[test]
    fn ignores_silent_flag_when_deciding_timestamp() {
        let (timestamp_us, new_anchor) =
            timestamp_for_packet(SILENT, 100_000_000, 480, 48000, 7_000_000, None, None);

        assert_eq!(
            timestamp_us, 10_000_000,
            "SILENT フラグはタイムスタンプの決定に影響しないこと"
        );
        assert_eq!(
            new_anchor,
            Some(anchor(480, 10_000_000)),
            "QPC 位置を使ったパケットを基準にすること"
        );
    }

    /// タイムスタンプエラーがあり基準がある場合は、QPC 位置を使わずに
    /// デバイス位置の差から補間し、そのパケットを新しい基準にすること
    #[test]
    fn interpolates_timestamp_when_timestamp_has_error() {
        let (timestamp_us, new_anchor) = timestamp_for_packet(
            TIMESTAMP_ERROR,
            100_000_000,
            960,
            48000,
            7_000_000,
            Some(anchor(480, 1_000_000)),
            None,
        );

        assert_eq!(
            timestamp_us, 1_010_000,
            "48000 Hz で 480 フレーム (10 ms) 後の時刻を補間すること"
        );
        assert_eq!(
            new_anchor,
            Some(anchor(960, 1_010_000)),
            "補間したパケットを基準にすること"
        );
    }

    /// タイムスタンプエラーがありデバイス位置が変わらない場合は、直前の
    /// タイムスタンプを返し、そのパケットを基準にすること
    #[test]
    fn interpolates_zero_elapsed_frames_when_timestamp_has_error() {
        let (timestamp_us, new_anchor) = timestamp_for_packet(
            TIMESTAMP_ERROR,
            0,
            480,
            48000,
            7_000_000,
            Some(anchor(480, 1_000_000)),
            None,
        );

        assert_eq!(
            timestamp_us, 1_000_000,
            "経過フレーム数が 0 のときは直前のタイムスタンプを返すこと"
        );
        assert_eq!(
            new_anchor,
            Some(anchor(480, 1_000_000)),
            "同じデバイス位置とタイムスタンプを基準にすること"
        );
    }

    /// タイムスタンプエラーがあり基準がない場合は、読み出し時刻を返して
    /// 基準を持たないこと
    #[test]
    fn falls_back_to_read_time_when_timestamp_has_error_without_reference() {
        let (timestamp_us, new_anchor) = timestamp_for_packet(
            TIMESTAMP_ERROR,
            100_000_000,
            960,
            48000,
            7_000_000,
            None,
            None,
        );

        assert_eq!(timestamp_us, 7_000_000, "読み出し時刻を返すこと");
        assert_eq!(new_anchor, None, "読み出し時刻を補間の基準にしないこと");
    }

    /// データの途切れがあり QPC 位置が使えない場合は、直前の基準をリセットして
    /// 読み出し時刻を返すこと
    #[test]
    fn resets_reference_on_data_discontinuity() {
        let (timestamp_us, new_anchor) = timestamp_for_packet(
            TIMESTAMP_ERROR | DATA_DISCONTINUITY,
            100_000_000,
            960,
            48000,
            7_000_000,
            Some(anchor(480, 1_000_000)),
            None,
        );

        assert_eq!(timestamp_us, 7_000_000, "補間せず読み出し時刻を返すこと");
        assert_eq!(new_anchor, None, "基準をリセットすること");
    }

    /// データの途切れがあっても QPC 位置が有効なら、QPC 位置を使い、
    /// そのパケットを新しい基準にすること
    #[test]
    fn uses_qpc_position_on_data_discontinuity() {
        let (timestamp_us, new_anchor) = timestamp_for_packet(
            DATA_DISCONTINUITY,
            100_000_000,
            960,
            48000,
            7_000_000,
            Some(anchor(480, 1_000_000)),
            None,
        );

        assert_eq!(timestamp_us, 10_000_000, "QPC 位置のマイクロ秒を使うこと");
        assert_eq!(
            new_anchor,
            Some(anchor(960, 10_000_000)),
            "QPC 位置を使ったパケットを基準にすること"
        );
    }

    /// 補間できない場合は読み出し時刻を返し、基準を持たないこと
    #[test]
    fn falls_back_to_read_time_when_interpolation_fails() {
        let last_anchor = Some(anchor(480, 1_000_000));

        for (sample_rate, device_position) in [(0, 960), (-48000, 960), (48000, 0)] {
            let (timestamp_us, new_anchor) = timestamp_for_packet(
                TIMESTAMP_ERROR,
                0,
                device_position,
                sample_rate,
                7_000_000,
                last_anchor,
                None,
            );

            assert_eq!(
                timestamp_us, 7_000_000,
                "sample_rate={sample_rate}, device_position={device_position} のとき読み出し時刻を返すこと"
            );
            assert_eq!(
                new_anchor, None,
                "sample_rate={sample_rate}, device_position={device_position} のとき基準をリセットすること"
            );
        }
    }

    /// 補間結果が i64 の範囲を超える場合は読み出し時刻を返すこと
    #[test]
    fn falls_back_to_read_time_when_interpolation_overflows() {
        let (timestamp_us, new_anchor) = timestamp_for_packet(
            TIMESTAMP_ERROR,
            0,
            48000,
            48000,
            7_000_000,
            Some(anchor(0, i64::MAX)),
            None,
        );

        assert_eq!(
            timestamp_us, 7_000_000,
            "オーバーフローする補間値ではなく読み出し時刻を返すこと"
        );
        assert_eq!(new_anchor, None, "基準をリセットすること");
    }

    /// QPC 位置をマイクロ秒へ変換すること
    #[test]
    fn converts_qpc_position_to_micros() {
        assert_eq!(qpc_position_to_micros(0), 0, "0 は 0 マイクロ秒になること");
        assert_eq!(
            qpc_position_to_micros(100_000),
            10_000,
            "100 ナノ秒単位の 10 ms が 10,000 マイクロ秒になること"
        );
        assert_eq!(
            qpc_position_to_micros(15),
            1,
            "100 ナノ秒未満の端数は切り捨てること"
        );
        assert_eq!(
            qpc_position_to_micros(u64::MAX),
            1_844_674_407_370_955_161,
            "u64 の最大値でも i64 の範囲に収まること"
        );
    }

    /// デバイス位置の差から経過時間を補間すること
    #[test]
    fn interpolates_ten_milliseconds() {
        assert_eq!(
            interpolate_timestamp_us(1_000_000, 480, 960, 48000),
            Some(1_010_000),
            "48000 Hz で 480 フレーム進むと 10 ms になること"
        );
    }

    /// デバイス位置が変わらない場合は、直前のタイムスタンプを返すこと
    #[test]
    fn returns_last_timestamp_when_elapsed_frames_are_zero() {
        assert_eq!(
            interpolate_timestamp_us(1_000_000, 480, 480, 48000),
            Some(1_000_000),
            "経過フレーム数が 0 のときは直前のタイムスタンプを返すこと"
        );
    }

    /// 直前のタイムスタンプがない場合と、直前より大きい場合はそのまま返すこと
    #[test]
    fn keeps_timestamp_when_it_increases() {
        assert_eq!(
            clamp_timestamp_us(1_000_000, None),
            1_000_000,
            "直前のタイムスタンプがない場合はそのまま返すこと"
        );
        assert_eq!(
            clamp_timestamp_us(1_000_001, Some(1_000_000)),
            1_000_001,
            "直前より大きい場合はそのまま返すこと"
        );
    }

    /// 直前のタイムスタンプ以下の場合は、直前の値に 1 マイクロ秒を加えた値を返すこと
    #[test]
    fn clamps_non_increasing_timestamp() {
        assert_eq!(
            clamp_timestamp_us(1_000_000, Some(1_000_000)),
            1_000_001,
            "直前と同じ場合は 1 マイクロ秒を加えること"
        );
        assert_eq!(
            clamp_timestamp_us(999_999, Some(1_000_000)),
            1_000_001,
            "直前より小さい場合は 1 マイクロ秒を加えること"
        );
    }

    /// 直前のタイムスタンプが i64::MAX の場合は i64::MAX を返すこと
    #[test]
    fn saturates_clamp_at_i64_max() {
        assert_eq!(
            clamp_timestamp_us(i64::MAX, Some(i64::MAX)),
            i64::MAX,
            "i64::MAX を超えないこと"
        );
    }

    /// サンプルレートが 0 以下、デバイス位置の逆転、計算オーバーフローの場合は
    /// 補間できないこと
    #[test]
    fn rejects_interpolation_when_input_is_invalid() {
        assert_eq!(
            interpolate_timestamp_us(1_000_000, 480, 960, 0),
            None,
            "sample_rate が 0 のときは補間しないこと"
        );
        assert_eq!(
            interpolate_timestamp_us(1_000_000, 480, 960, -48000),
            None,
            "sample_rate が負のときは補間しないこと"
        );
        assert_eq!(
            interpolate_timestamp_us(1_000_000, 960, 480, 48000),
            None,
            "デバイス位置が逆転しているときは補間しないこと"
        );
        assert_eq!(
            interpolate_timestamp_us(i64::MAX, 0, 48000, 48000),
            None,
            "補間結果が i64 の範囲を超えるときは補間しないこと"
        );
    }
}
