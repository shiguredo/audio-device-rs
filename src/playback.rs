//! macOS/Linux 用オーディオ再生
//!
//! 現在未実装

use crate::device::AudioFormat;
use crate::error::{Error, Result};

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
        let frames = data.len() as i32 / channels;
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
        let frames = data.len() as i32 / channels;
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

/// オーディオ再生
///
/// 現在 macOS/Linux では未実装
pub struct AudioPlayback {
    config: AudioPlaybackConfig,
}

impl AudioPlayback {
    /// 新しい AudioPlayback を作成
    ///
    /// 現在 macOS/Linux では未実装
    pub fn new<F>(_config: AudioPlaybackConfig, _callback: F) -> Result<Self>
    where
        F: Fn() -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        // TODO: macOS/Linux での音声再生を実装
        Err(Error::SessionCreateFailed)
    }

    /// 再生を開始
    pub fn start(&mut self) -> Result<()> {
        Err(Error::SessionStartFailed)
    }

    /// 再生を停止
    pub fn stop(&mut self) {
        // No-op
    }

    /// 設定を取得
    pub fn config(&self) -> &AudioPlaybackConfig {
        &self.config
    }

    /// 実際のサンプルレートを取得
    pub fn sample_rate(&self) -> i32 {
        self.config.sample_rate
    }

    /// 実際のチャンネル数を取得
    pub fn channels(&self) -> i32 {
        self.config.channels
    }
}
