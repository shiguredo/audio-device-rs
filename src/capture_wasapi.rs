//! Windows 用オーディオキャプチャ (WASAPI)

use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use windows::{
    Win32::Foundation::*, Win32::Media::Audio::*, Win32::Media::KernelStreaming::*,
    Win32::Media::Multimedia::*, Win32::System::Com::*, Win32::System::Performance::*,
    Win32::System::Threading::*,
};

use crate::common::{AudioCaptureConfig, AudioDeviceType, AudioFormat, AudioFrame};
use crate::device_wasapi::{SendHandle, SendPtr, get_device_by_id};
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
        let device = get_device_by_id(config.device_id.as_deref(), AudioDeviceType::Input)?;

        unsafe {
            let audio_client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|_| Error::DeviceAccessDenied)?;

            let mix_format = audio_client
                .GetMixFormat()
                .map_err(|_| Error::DeviceAccessDenied)?;
            let wave_format = &*mix_format;

            let format_tag = wave_format.wFormatTag;
            let format = if format_tag == WAVE_FORMAT_IEEE_FLOAT as u16
                || (format_tag == WAVE_FORMAT_EXTENSIBLE as u16 && {
                    let ext = mix_format as *const WAVEFORMATEXTENSIBLE;
                    std::ptr::addr_of!((*ext).SubFormat).read_unaligned()
                        == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
                }) {
                AudioFormat::F32
            } else {
                AudioFormat::S16
            };

            let sample_rate = wave_format.nSamplesPerSec as i32;
            let channels = wave_format.nChannels as i32;

            CoTaskMemFree(Some(mix_format as *const _));

            audio_client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    100_000, // 10ms
                    0,
                    mix_format,
                    Some(std::ptr::null()),
                )
                .map_err(|_| Error::SessionCreateFailed)?;

            let _buffer_frame_count = audio_client
                .GetBufferSize()
                .map_err(|_| Error::SessionCreateFailed)?
                as u32;

            let capture_client: IAudioCaptureClient = audio_client
                .GetService()
                .map_err(|_| Error::SessionCreateFailed)?;

            let event_handle =
                CreateEventW(None, false, false, None).map_err(|_| Error::SessionCreateFailed)?;

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

        unsafe {
            session
                .audio_client
                .Start()
                .map_err(|_| Error::SessionStartFailed)?;
        }

        context.running.store(true, Ordering::Release);

        let capture_client = SendPtr(session.capture_client.clone());
        let event_handle = SendHandle(session.event_handle);
        let format = session.format;
        let sample_rate = session.sample_rate;
        let channels = session.channels;
        let context = Arc::clone(context);

        let handle = thread::Builder::new()
            .name("audio-capture".to_string())
            .spawn(move || {
                capture_thread_func(
                    capture_client.into_inner(),
                    event_handle.into_inner(),
                    format,
                    sample_rate,
                    channels,
                    context,
                );
            })
            .map_err(|_| {
                unsafe {
                    let _ = session.audio_client.Stop();
                }
                Error::SessionCreateFailed
            })?;

        self.capture_thread = Some(handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(context) = &self.context
            && context.running.load(Ordering::Acquire)
        {
            context.running.store(false, Ordering::Release);
            if let Some(session) = &self.session {
                unsafe {
                    let _ = SetEvent(session.event_handle);
                }
            }
            if let Some(handle) = self.capture_thread.take() {
                let _ = handle.join();
            }
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

unsafe impl Send for WasapiCaptureImpl {}
unsafe impl Sync for WasapiCaptureImpl {}

fn capture_thread_func(
    capture_client: IAudioCaptureClient,
    event_handle: HANDLE,
    format: AudioFormat,
    sample_rate: i32,
    channels: i32,
    context: Arc<WasapiCaptureContext>,
) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut frequency = 0i64;
        let _ = QueryPerformanceFrequency(&mut frequency);

        while context.running.load(Ordering::Acquire) {
            let wait_result = WaitForSingleObject(event_handle, 10);
            if !context.running.load(Ordering::Acquire) {
                break;
            }
            if wait_result != WAIT_OBJECT_0 && wait_result != WAIT_TIMEOUT {
                continue;
            }

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
                    let mut counter = 0i64;
                    let _ = QueryPerformanceCounter(&mut counter);
                    let timestamp_us = if frequency > 0 {
                        (counter * 1_000_000) / frequency
                    } else {
                        0
                    };

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
        CoUninitialize();
    }
}
