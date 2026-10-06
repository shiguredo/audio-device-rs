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
    /// 符号付き 16-bit 整数
    S16,
    /// 32-bit 浮動小数点数
    F32,
}

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
impl AudioDeviceType {
    pub(crate) fn from_ffi(device_type: i32) -> Result<Self> {
        const OUTPUT: i32 = ffi::AUDIO_DEVICE_TYPE_OUTPUT as i32;
        const INPUT: i32 = ffi::AUDIO_DEVICE_TYPE_INPUT as i32;
        match device_type {
            OUTPUT => Ok(AudioDeviceType::Output),
            INPUT => Ok(AudioDeviceType::Input),
            other => Err(Error::UnknownDeviceType(other)),
        }
    }
}

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
impl AudioFormat {
    /// 1 サンプルあたりのバイト数を返す
    pub(crate) fn bytes_per_sample(self) -> usize {
        match self {
            AudioFormat::S16 => 2,
            AudioFormat::F32 => 4,
        }
    }
}

#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]
impl AudioFormat {
    pub(crate) fn from_ffi(format: i32) -> Result<Self> {
        const F32: i32 = ffi::AUDIO_FORMAT_F32 as i32;
        const S16: i32 = ffi::AUDIO_FORMAT_S16 as i32;
        match format {
            F32 => Ok(AudioFormat::F32),
            S16 => Ok(AudioFormat::S16),
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
    /// デバイス ID（None の場合はデフォルトデバイス）
    pub device_id: Option<String>,
    /// サンプルレート
    pub sample_rate: i32,
    /// チャンネル数
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
#[derive(Debug, Clone)]
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
#[cfg(any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi))]
pub(crate) fn write_playback_frame_to_buffer(
    src_data: &[u8],
    src_format: AudioFormat,
    dst: &mut [u8],
    dst_format: AudioFormat,
    dst_channels: usize,
) -> i32 {
    match (src_format, dst_format) {
        (AudioFormat::S16, AudioFormat::S16) | (AudioFormat::F32, AudioFormat::F32) => {
            // 同一フォーマットの場合はそのままコピーする
            let copy_len = src_data.len().min(dst.len());
            let bytes_per_sample = src_format.bytes_per_sample();
            let sample_size = dst_channels * bytes_per_sample;
            let copy_frames = copy_len / sample_size;
            let copy_bytes = copy_frames * sample_size;
            dst[..copy_bytes].copy_from_slice(&src_data[..copy_bytes]);
            if copy_bytes < dst.len() {
                dst[copy_bytes..].fill(0);
            }
            copy_frames as i32
        }
        (AudioFormat::F32, AudioFormat::S16) => {
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
        }
        (AudioFormat::S16, AudioFormat::F32) => {
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
        }
    }
}

#[cfg(all(
    test,
    any(enable_coreaudio, enable_pulse, enable_pipewire, enable_wasapi)
))]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // 同一フォーマット (S16 → S16 / F32 → F32)
    // -----------------------------------------------------------------------

    /// 空の src_data では書き込みフレーム数 0 でバッファ全体がゼロ埋めされる
    #[test]
    fn empty_src_data_fills_dst_with_zero() {
        let mut dst = vec![0xCCu8; 60];
        let frames =
            write_playback_frame_to_buffer(&[], AudioFormat::S16, &mut dst, AudioFormat::S16, 2);
        assert_eq!(frames, 0);
        assert!(
            dst.iter().all(|&b| b == 0),
            "バッファ全体がゼロ埋めされること"
        );
    }

    /// 変換後のフレーム数が dst_channels で割り切れない場合は切り捨てる
    #[test]
    fn f32_to_s16_truncates_partial_frame() {
        // 3ch 分の F32 データ 7 サンプル (2 フレーム + 1 サンプル)
        let src: Vec<f32> = vec![0.0, 0.25, -0.25, 0.5, -0.5, 1.0, -1.0];
        let src_bytes: Vec<u8> = src.iter().flat_map(|f| f.to_le_bytes()).collect();
        // 3ch 分の S16 バッファ (7 サンプルぶん)
        let mut dst = vec![0xCCu8; 14];
        let frames = write_playback_frame_to_buffer(
            &src_bytes,
            AudioFormat::F32,
            &mut dst,
            AudioFormat::S16,
            3,
        );
        assert_eq!(frames, 2, "7 / 3 = 2 フレームに切り捨てられること");
        // 先頭 2 サンプルだけ検証する (3 サンプル目はフレームに満たない)
        let written0 = i16::from_le_bytes([dst[0], dst[1]]);
        let written1 = i16::from_le_bytes([dst[2], dst[3]]);
        assert_eq!(written0, 0);
        assert_eq!(written1, 8191);
    }

    /// F32 → F32 の同一フォーマットで不足分がゼロ埋めされる
    #[test]
    fn same_format_f32_pads_with_silence() {
        // 2ch で 2 フレームぶんのデータ
        let src: Vec<f32> = vec![0.5, -0.5, 0.25, -0.25];
        let src_bytes: Vec<u8> = src.iter().flat_map(|f| f.to_le_bytes()).collect();
        // 2ch で 3 フレームぶんのバッファ
        let mut dst = vec![0xCCu8; 24];
        let frames = write_playback_frame_to_buffer(
            &src_bytes,
            AudioFormat::F32,
            &mut dst,
            AudioFormat::F32,
            2,
        );
        assert_eq!(frames, 2, "データぶんの 2 フレームが書き込まれること");
        // 書き込まれた部分は元データと一致する
        assert_eq!(&dst[..src_bytes.len()], &src_bytes[..]);
        // 残りはゼロ埋めされる
        assert!(
            dst[src_bytes.len()..].iter().all(|&b| b == 0),
            "不足分がゼロ埋めされること"
        );
    }

    /// S16 → F32 のマルチチャンネル変換で全サンプルが変換される
    #[test]
    fn s16_to_f32_converts_multichannel() {
        // 2ch で 2 フレームぶんのデータ
        let src: Vec<i16> = vec![32767, -32768, 16384, -16384];
        let src_bytes: Vec<u8> = src.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut dst = vec![0u8; 16];
        let frames = write_playback_frame_to_buffer(
            &src_bytes,
            AudioFormat::S16,
            &mut dst,
            AudioFormat::F32,
            2,
        );
        assert_eq!(frames, 2, "2 フレームが書き込まれること");
        for (i, exp) in [32767.0f32 / 32768.0, -1.0, 0.5, -0.5].iter().enumerate() {
            let offset = i * 4;
            let written = f32::from_le_bytes([
                dst[offset],
                dst[offset + 1],
                dst[offset + 2],
                dst[offset + 3],
            ]);
            assert!(
                (written - exp).abs() < 1e-7,
                "サンプル {i}: 期待 {exp}, 実際 {written}"
            );
        }
    }

    /// F32 → S16 変換で NaN と ±Inf が期待どおりに扱われる
    #[test]
    fn f32_to_s16_handles_nan_and_infinity() {
        let src: Vec<f32> = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0];
        let src_bytes: Vec<u8> = src.iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut dst = vec![0u8; 8];
        let frames = write_playback_frame_to_buffer(
            &src_bytes,
            AudioFormat::F32,
            &mut dst,
            AudioFormat::S16,
            1,
        );
        assert_eq!(frames, 4);
        // NaN → 0, +Inf → clamp で 32767, -Inf → clamp で -32767, 0.0 → 0
        let expected: [i16; 4] = [0, 32767, -32767, 0];
        for (i, exp) in expected.iter().enumerate() {
            let offset = i * 2;
            let written = i16::from_le_bytes([dst[offset], dst[offset + 1]]);
            assert_eq!(written, *exp, "サンプル {i} が一致しない");
        }
    }
}
