//! Windows 用オーディオ再生 (WASAPI)

use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use windows::{
    Win32::Foundation::*, Win32::Media::Audio::*, Win32::Media::KernelStreaming::*,
    Win32::Media::Multimedia::*, Win32::System::Com::*, Win32::System::Threading::*,
};

use crate::common::{AudioDeviceType, AudioFormat, AudioPlaybackConfig, PlaybackFrame};
use crate::device_wasapi::{SendHandle, SendPtr, get_device_by_id};
use crate::error::{Error, Result};

pub(crate) struct PlaybackContext {
    pub(crate) callback: Box<dyn Fn() -> Option<PlaybackFrame> + Send + Sync>,
    pub(crate) running: AtomicBool,
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

unsafe impl Send for SendPtr<IAudioRenderClient> {}
unsafe impl Send for SendPtr<IAudioClient> {}

pub(crate) struct WasapiPlaybackImpl {
    session: Option<SessionData>,
    context: Option<Arc<PlaybackContext>>,
    playback_thread: Option<thread::JoinHandle<()>>,
    config: AudioPlaybackConfig,
    actual_sample_rate: i32,
    actual_channels: i32,
}

impl WasapiPlaybackImpl {
    pub fn new<F>(config: AudioPlaybackConfig, callback: F) -> Result<Self>
    where
        F: Fn() -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        let device = get_device_by_id(config.device_id.as_deref(), AudioDeviceType::Output)?;

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
                    100_000,
                    0,
                    mix_format,
                    Some(std::ptr::null()),
                )
                .map_err(|_| Error::SessionCreateFailed)?;

            let buffer_frames = audio_client
                .GetBufferSize()
                .map_err(|_| Error::SessionCreateFailed)?;

            let render_client: IAudioRenderClient = audio_client
                .GetService()
                .map_err(|_| Error::SessionCreateFailed)?;

            let event_handle =
                CreateEventW(None, false, false, None).map_err(|_| Error::SessionCreateFailed)?;

            audio_client.SetEventHandle(event_handle).map_err(|_| {
                let _ = CloseHandle(event_handle);
                Error::SessionCreateFailed
            })?;

            let context = Arc::new(PlaybackContext {
                callback: Box::new(callback),
                running: AtomicBool::new(false),
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
                context: Some(context),
                playback_thread: None,
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

        let render_client = SendPtr(session.render_client.clone());
        let audio_client = SendPtr(session.audio_client.clone());
        let event_handle = SendHandle(session.event_handle);
        let format = session.format;
        let sample_rate = session.sample_rate;
        let channels = session.channels;
        let buffer_frames = session.buffer_frames;
        let context = Arc::clone(context);

        let handle = thread::Builder::new()
            .name("audio-playback".to_string())
            .spawn(move || {
                playback_thread_func(
                    render_client.into_inner(),
                    audio_client.into_inner(),
                    event_handle.into_inner(),
                    format,
                    sample_rate,
                    channels,
                    buffer_frames,
                    context,
                );
            })
            .map_err(|_| {
                unsafe {
                    let _ = session.audio_client.Stop();
                }
                Error::SessionCreateFailed
            })?;

        self.playback_thread = Some(handle);
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
            if let Some(handle) = self.playback_thread.take() {
                let _ = handle.join();
            }
            if let Some(session) = &self.session {
                unsafe {
                    let _ = session.audio_client.Stop();
                }
            }
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

unsafe impl Send for WasapiPlaybackImpl {}
unsafe impl Sync for WasapiPlaybackImpl {}

#[expect(clippy::too_many_arguments)]
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
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        while context.running.load(Ordering::Acquire) {
            let wait_result = WaitForSingleObject(event_handle, 10);
            if !context.running.load(Ordering::Acquire) {
                break;
            }
            if wait_result != WAIT_OBJECT_0 && wait_result != WAIT_TIMEOUT {
                continue;
            }

            let padding = match audio_client.GetCurrentPadding() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let frames_available = buffer_frames.saturating_sub(padding);
            if frames_available == 0 {
                continue;
            }

            let frame_opt = (context.callback)();

            let data_ptr = match render_client.GetBuffer(frames_available) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if let Some(frame) = frame_opt {
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

                if frame.format == AudioFormat::F32 && format == AudioFormat::S16 {
                    let src_count = frame.data.len() / 4;
                    let dst_s16 =
                        std::slice::from_raw_parts_mut(data_ptr as *mut i16, buffer_size / 2);
                    let copy_len = src_count.min(dst_s16.len());
                    let src_ptr = frame.data.as_ptr() as *const f32;
                    for (i, dst) in dst_s16.iter_mut().enumerate().take(copy_len) {
                        let sample = src_ptr.add(i).read_unaligned();
                        *dst = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                    }
                    dst_s16[copy_len..].fill(0);
                } else if frame.format == AudioFormat::S16 && format == AudioFormat::F32 {
                    let src_count = frame.data.len() / 2;
                    let dst_f32 =
                        std::slice::from_raw_parts_mut(data_ptr as *mut f32, buffer_size / 4);
                    let copy_len = src_count.min(dst_f32.len());
                    let src_ptr = frame.data.as_ptr() as *const i16;
                    for (i, dst) in dst_f32.iter_mut().enumerate().take(copy_len) {
                        let sample = src_ptr.add(i).read_unaligned();
                        *dst = sample as f32 / 32768.0;
                    }
                    dst_f32[copy_len..].fill(0.0);
                } else {
                    let copy_len = frame.data.len().min(buffer_size);
                    ptr::copy_nonoverlapping(frame.data.as_ptr(), data_ptr, copy_len);
                    if copy_len < buffer_size {
                        ptr::write_bytes(data_ptr.add(copy_len), 0, buffer_size - copy_len);
                    }
                }
                let _ = render_client.ReleaseBuffer(frames_available, 0);
            } else {
                let _ = render_client
                    .ReleaseBuffer(frames_available, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32);
            }
        }
        CoUninitialize();
    }
}
