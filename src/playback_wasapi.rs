//! Windows 用オーディオ再生 (WASAPI)

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use windows::{
    Win32::Foundation::*, Win32::Media::Audio::*, Win32::System::Com::*,
    Win32::System::Threading::*,
};

use crate::common::{
    AudioDeviceType, AudioFormat, AudioPlaybackConfig, PlaybackFrame,
    write_playback_frame_to_buffer,
};
use crate::device_wasapi::{ComGuard, SendHandle, SendPtr, build_wave_format, get_device_by_id};
use crate::error::{Error, Result};

struct PlaybackContext {
    callback: Box<dyn Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send>,
}

// SAFETY: PlaybackContext は WasapiPlaybackImpl の Box に保持され、再生スレッドの
// 実行中は drop されない。stop はスレッドを join してから戻るため、スレッドは
// join 後にコンテキストへアクセスしない。コールバックは再生スレッドからのみ
// 直列に呼ばれ、その実行中に WasapiPlaybackImpl 側が context へアクセスすることも
// ない。
unsafe impl Send for SendPtr<*const PlaybackContext> {}

struct SessionData {
    audio_client: IAudioClient,
    render_client: IAudioRenderClient,
    event_handle: HANDLE,
    format: AudioFormat,
    sample_rate: i32,
    channels: i32,
    buffer_frames: u32,
}

// SAFETY: IAudioRenderClient / IAudioClient は COM の MTA (COINIT_MULTITHREADED) で初期化しており、
// MTA オブジェクトはスレッド間で安全に移送できる。
unsafe impl Send for SendPtr<IAudioRenderClient> {}
unsafe impl Send for SendPtr<IAudioClient> {}

/// オーディオ再生
pub(crate) struct WasapiPlaybackImpl {
    session: Option<SessionData>,
    /// ユーザーコールバックを保持するコンテキスト
    context: Box<PlaybackContext>,
    /// 再生スレッドと停止処理で共有する実行状態
    running: Arc<AtomicBool>,
    playback_thread: Option<thread::JoinHandle<()>>,
    config: AudioPlaybackConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
}

impl WasapiPlaybackImpl {
    /// 新しい AudioPlayback を作成する
    ///
    /// `callback` はフレームデータを要求されたときに呼ばれる。
    /// データがない場合は `None` を返すと無音が再生される。
    pub fn new<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn(i32, i32, i32) -> Option<PlaybackFrame> + Send + 'static,
    {
        // デバイスを取得（再生なので出力デバイス）
        let device = get_device_by_id(config.device_id.as_deref(), AudioDeviceType::Output)?;

        unsafe {
            // オーディオクライアントを取得
            let audio_client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|_| Error::DeviceAccessDenied)?;

            // 要求する形式は F32 に固定する。S16 のフレームも F32 へは無損失で変換できる。
            // 0 以下の値は既定値に丸める
            let sample_rate = if config.sample_rate <= 0 {
                48000
            } else {
                config.sample_rate as u32
            };
            let channels = if config.channels <= 0 {
                2
            } else {
                config.channels
            };
            let channels = u16::try_from(channels).map_err(|_| Error::InvalidAudioFormat)?;
            let client_format = build_wave_format(AudioFormat::F32, sample_rate, channels)?;
            let sample_rate = client_format.nSamplesPerSec as i32;
            let channels = client_format.nChannels as i32;

            // デバイスの mix format への変換は共有モードの形式変換に任せる
            let init_result = audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                100_000, // 100 ナノ秒単位で 10ms を指定する
                0,
                &client_format,
                Some(std::ptr::null()),
            );
            init_result.map_err(|_| Error::SessionCreateFailed)?;

            let format = AudioFormat::F32;

            // バッファサイズを取得
            let buffer_frames = audio_client
                .GetBufferSize()
                .map_err(|_| Error::SessionCreateFailed)?;

            // レンダークライアントを取得
            let render_client: IAudioRenderClient = audio_client
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

            let context = Box::new(PlaybackContext {
                callback: Box::new(callback),
            });

            Ok(Self {
                session: Some(SessionData {
                    audio_client,
                    render_client,
                    event_handle,
                    format,
                    sample_rate,
                    channels,
                    buffer_frames,
                }),
                context,
                running: Arc::new(AtomicBool::new(false)),
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

        // スレッド生成前に running フラグを立てる
        self.running.store(true, Ordering::Release);

        // コンテキストは self に残したまま、そのポインタだけをスレッドへ渡す。
        // Box の内容は WasapiPlaybackImpl が drop されるまで移動しない。stop は
        // スレッドを join してから戻り、Box はその後 drop されるため、ポインタは
        // スレッドの実行中ずっと有効である。
        let context = SendPtr(std::ptr::from_ref(&*self.context));
        let running = Arc::clone(&self.running);

        // 再生スレッドを開始
        let handle = thread::Builder::new()
            .name("audio-playback".to_string())
            .spawn(move || {
                let _com_guard = ComGuard::init_mta();
                playback_thread_func(
                    render_client.into_inner(),
                    audio_client.into_inner(),
                    event_handle.into_inner(),
                    format,
                    sample_rate,
                    channels,
                    buffer_frames,
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

        self.playback_thread = Some(handle);
        Ok(())
    }

    /// 再生を停止
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

impl Drop for WasapiPlaybackImpl {
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
unsafe impl Send for WasapiPlaybackImpl {}

/// 再生スレッド関数
///
/// `context` は `PlaybackContext` を指す。この関数が戻るまで、そのコンテキストが
/// 存続することを前提とする。
#[expect(clippy::too_many_arguments)]
fn playback_thread_func(
    render_client: IAudioRenderClient,
    audio_client: IAudioClient,
    event_handle: HANDLE,
    format: AudioFormat,
    sample_rate: i32,
    channels: i32,
    buffer_frames: u32,
    context: *const PlaybackContext,
    running: Arc<AtomicBool>,
) {
    // SAFETY: context は WasapiPlaybackImpl の Box が保持する PlaybackContext を
    // 指す。stop はこのスレッドを join してから戻るため、このスレッドの実行中は
    // コンテキストが生存する。コールバックはこのスレッドからしか呼ばれない。
    let context = unsafe { &*context };

    // running フラグが false になるまでループする
    while running.load(Ordering::Acquire) {
        // バッファに空きができるまでイベントを待機する
        let wait_result = unsafe { WaitForSingleObject(event_handle, 10) };
        if !running.load(Ordering::Acquire) {
            break;
        }
        if wait_result != WAIT_OBJECT_0 && wait_result != WAIT_TIMEOUT {
            continue;
        }

        // バッファの空きフレーム数を計算する
        let padding = match unsafe { audio_client.GetCurrentPadding() } {
            Ok(p) => p,
            Err(_) => continue,
        };
        let frames_available = buffer_frames.saturating_sub(padding);
        if frames_available == 0 {
            continue;
        }

        // レンダリングバッファを取得する
        let data_ptr = match unsafe { render_client.GetBuffer(frames_available) } {
            Ok(p) => p,
            Err(_) => continue,
        };

        // ユーザーコールバックからフレームデータを取得する
        let frame_opt = (context.callback)(frames_available as i32, channels, sample_rate);

        if let Some(frame) = frame_opt {
            // フレームデータをバッファに変換して書き込む
            let bytes_per_sample: usize = format.bytes_per_sample();
            let buffer_size = match (frames_available as usize)
                .checked_mul(channels as usize)
                .and_then(|n| n.checked_mul(bytes_per_sample))
            {
                Some(size) => size,
                None => {
                    unsafe {
                        let _ = render_client.ReleaseBuffer(frames_available, 0);
                    }
                    continue;
                }
            };

            let dst = unsafe { std::slice::from_raw_parts_mut(data_ptr, buffer_size) };
            write_playback_frame_to_buffer(
                &frame.data,
                frame.format,
                dst,
                format,
                channels as usize,
            );
            unsafe {
                let _ = render_client.ReleaseBuffer(frames_available, 0);
            }
        } else {
            // データがない場合は無音で埋める
            unsafe {
                let _ = render_client
                    .ReleaseBuffer(frames_available, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32);
            }
        }
    }
}
