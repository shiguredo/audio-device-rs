//! macOS/Linux 用オーディオ再生
//!
//! 現在未実装

use crate::common::{AudioPlaybackConfig, PlaybackFrame};
use crate::error::{Error, Result};

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
