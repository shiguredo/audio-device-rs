//! プラットフォーム非依存の共通オーディオ型定義

use crate::error::{Error, Result};

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
use crate::ffi;

/// オーディオデバイスの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDeviceType {
    /// 入力デバイス（マイク）
    Input,
    /// 出力デバイス（スピーカー）
    Output,
}

/// オーディオフォーマット
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    /// Signed 16-bit integer
    S16,
    /// 32-bit float
    F32,
}

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
impl AudioDeviceType {
    pub(crate) fn from_ffi(device_type: i32) -> Result<Self> {
        match device_type {
            x if x == ffi::AUDIO_DEVICE_TYPE_OUTPUT as i32 => Ok(AudioDeviceType::Output),
            x if x == ffi::AUDIO_DEVICE_TYPE_INPUT as i32 => Ok(AudioDeviceType::Input),
            other => Err(Error::UnknownDeviceType(other)),
        }
    }
}

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
impl AudioFormat {
    pub(crate) fn from_ffi(format: i32) -> Result<Self> {
        match format {
            x if x == ffi::AUDIO_FORMAT_F32 as i32 => Ok(AudioFormat::F32),
            x if x == ffi::AUDIO_FORMAT_S16 as i32 => Ok(AudioFormat::S16),
            other => Err(Error::UnknownFormat(other)),
        }
    }
}

/// オーディオフレームデータ
pub struct AudioFrame<'a> {
    /// PCM データ
    pub data: &'a [u8],
    /// サンプルフレーム数
    pub frames: i32,
    /// チャンネル数
    pub channels: i32,
    /// サンプルレート
    pub sample_rate: i32,
    /// オーディオフォーマット
    pub format: AudioFormat,
    /// タイムスタンプ（マイクロ秒）
    pub timestamp_us: i64,
}

impl<'a> AudioFrame<'a> {
    /// 参照データを所有データに変換する
    pub fn to_owned(&self) -> AudioFrameOwned {
        AudioFrameOwned {
            data: self.data.to_vec(),
            frames: self.frames,
            channels: self.channels,
            sample_rate: self.sample_rate,
            format: self.format,
            timestamp_us: self.timestamp_us,
        }
    }

    /// S16 フォーマットとしてデータを取得
    pub fn as_s16(&self) -> Option<&[i16]> {
        if self.format != AudioFormat::S16 {
            return None;
        }
        if self.frames <= 0 || self.channels <= 0 {
            return None;
        }
        let len = (self.frames as usize).checked_mul(self.channels as usize)?;
        let required_bytes = len.checked_mul(std::mem::size_of::<i16>())?;
        if self.data.len() < required_bytes {
            return None;
        }
        if !(self.data.as_ptr() as usize).is_multiple_of(std::mem::align_of::<i16>()) {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(self.data.as_ptr() as *const i16, len) })
    }

    /// F32 フォーマットとしてデータを取得
    pub fn as_f32(&self) -> Option<&[f32]> {
        if self.format != AudioFormat::F32 {
            return None;
        }
        if self.frames <= 0 || self.channels <= 0 {
            return None;
        }
        let len = (self.frames as usize).checked_mul(self.channels as usize)?;
        let required_bytes = len.checked_mul(std::mem::size_of::<f32>())?;
        if self.data.len() < required_bytes {
            return None;
        }
        if !(self.data.as_ptr() as usize).is_multiple_of(std::mem::align_of::<f32>()) {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(self.data.as_ptr() as *const f32, len) })
    }
}

/// オーディオフレームデータ (所有)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrameOwned {
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
    /// タイムスタンプ（マイクロ秒）
    pub timestamp_us: i64,
}

impl AudioFrameOwned {
    /// 参照フレームとして取得する
    pub fn as_frame(&self) -> AudioFrame<'_> {
        AudioFrame {
            data: &self.data,
            frames: self.frames,
            channels: self.channels,
            sample_rate: self.sample_rate,
            format: self.format,
            timestamp_us: self.timestamp_us,
        }
    }

    /// S16 フォーマットとしてデータを取得
    pub fn as_s16(&self) -> Option<&[i16]> {
        self.as_frame().as_s16().map(|s| {
            // SAFETY: as_frame() は self.data を参照しており、self の借用中は有効
            unsafe { std::slice::from_raw_parts(s.as_ptr(), s.len()) }
        })
    }

    /// F32 フォーマットとしてデータを取得
    pub fn as_f32(&self) -> Option<&[f32]> {
        self.as_frame().as_f32().map(|s| {
            // SAFETY: as_frame() は self.data を参照しており、self の借用中は有効
            unsafe { std::slice::from_raw_parts(s.as_ptr(), s.len()) }
        })
    }
}

/// オーディオキャプチャ設定
pub struct AudioCaptureConfig {
    pub device_id: Option<String>,
    pub sample_rate: i32,
    pub channels: i32,
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            sample_rate: 48000,
            channels: 1,
        }
    }
}

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
        let frames = i32::try_from(data.len())? / channels;
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
        let frames = i32::try_from(data.len())? / channels;
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

// ---------------------------------------------------------------------------
// 再生用フォーマット変換共通関数
// ---------------------------------------------------------------------------

/// 再生フレームのバイトデータを、指定先フォーマットに変換してバッファに書き込む。
///
/// フォーマットが一致する場合はそのままコピー、異なる場合は変換する。
/// バッファに満たない部分はゼロで埋める。
/// 書き込んだフレーム数を返す。
#[allow(dead_code)]
pub(crate) fn write_playback_frame_to_buffer(
    src_data: &[u8],
    src_format: AudioFormat,
    dst: &mut [u8],
    dst_format: AudioFormat,
    dst_channels: usize,
) -> i32 {
    if src_format == dst_format {
        // 同一フォーマットの場合はそのままコピーする
        let copy_len = src_data.len().min(dst.len());
        let bytes_per_sample = match src_format {
            AudioFormat::S16 => 2,
            AudioFormat::F32 => 4,
        };
        let sample_size = dst_channels * bytes_per_sample;
        let copy_frames = copy_len / sample_size;
        let copy_bytes = copy_frames * sample_size;
        dst[..copy_bytes].copy_from_slice(&src_data[..copy_bytes]);
        if copy_bytes < dst.len() {
            dst[copy_bytes..].fill(0);
        }
        copy_frames as i32
    } else if src_format == AudioFormat::F32 && dst_format == AudioFormat::S16 {
        // F32 → S16 変換
        // Vec<u8> のアライメントは 1 なので read_unaligned / write_unaligned で読み書きする
        let src_count = src_data.len() / 4;
        let dst_count = dst.len() / 2;
        let copy_len = src_count.min(dst_count);
        let src_ptr = src_data.as_ptr() as *const f32;
        unsafe {
            let dst_ptr = dst.as_mut_ptr() as *mut i16;
            for i in 0..copy_len {
                let sample = src_ptr.add(i).read_unaligned();
                // NaN は 0 に、±Inf は clamp で ±1 に収める
                let clamped = if sample.is_nan() {
                    0.0
                } else {
                    sample.clamp(-1.0, 1.0)
                };
                dst_ptr.add(i).write_unaligned((clamped * 32767.0) as i16);
            }
            let written_bytes = copy_len * 2;
            if written_bytes < dst.len() {
                dst[written_bytes..].fill(0);
            }
        }
        (copy_len as i32) / dst_channels as i32
    } else if src_format == AudioFormat::S16 && dst_format == AudioFormat::F32 {
        // S16 → F32 変換
        // Vec<u8> のアライメントは 1 なので read_unaligned / write_unaligned で読み書きする
        let src_count = src_data.len() / 2;
        let dst_count = dst.len() / 4;
        let copy_len = src_count.min(dst_count);
        let src_ptr = src_data.as_ptr() as *const i16;
        unsafe {
            let dst_ptr = dst.as_mut_ptr() as *mut f32;
            for i in 0..copy_len {
                let sample = src_ptr.add(i).read_unaligned();
                dst_ptr.add(i).write_unaligned(sample as f32 / 32768.0);
            }
            let written_bytes = copy_len * 4;
            if written_bytes < dst.len() {
                dst[written_bytes..].fill(0);
            }
        }
        (copy_len as i32) / dst_channels as i32
    } else {
        0
    }
}
