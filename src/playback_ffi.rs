//! macOS / Linux 共通のオーディオ再生実装。
//!
//! 現時点では macos/ffi の再生は未実装のため、全ての操作がエラーを返すスタブ。

use crate::common::{AudioPlaybackConfig, PlaybackFrame};
use crate::error::{Error, Result};

/// バックエンド固有の FFI 関数テーブル（将来の拡張用）。
struct PlaybackOps {}

pub(crate) struct FfiPlaybackImpl {
    _ops: &'static PlaybackOps,
    config: AudioPlaybackConfig,
}

impl FfiPlaybackImpl {
    #[cfg(enable_coreaudio)]
    pub(crate) fn new_coreaudio<F>(_config: AudioPlaybackConfig, _callback: F) -> Result<Self>
    where
        F: Fn() -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Err(Error::SessionCreateFailed)
    }

    #[cfg(enable_pulse)]
    pub(crate) fn new_pulse<F>(_config: AudioPlaybackConfig, _callback: F) -> Result<Self>
    where
        F: Fn() -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Err(Error::SessionCreateFailed)
    }

    #[cfg(enable_pipewire)]
    pub(crate) fn new_pipewire<F>(_config: AudioPlaybackConfig, _callback: F) -> Result<Self>
    where
        F: Fn() -> Option<PlaybackFrame> + Send + Sync + 'static,
    {
        Err(Error::SessionCreateFailed)
    }

    pub fn start(&mut self) -> Result<()> {
        Err(Error::SessionStartFailed)
    }

    pub fn stop(&mut self) {}

    pub fn config(&self) -> &AudioPlaybackConfig {
        &self.config
    }

    pub fn sample_rate(&self) -> i32 {
        self.config.sample_rate
    }

    pub fn channels(&self) -> i32 {
        self.config.channels
    }
}

unsafe impl Send for FfiPlaybackImpl {}
unsafe impl Sync for FfiPlaybackImpl {}
