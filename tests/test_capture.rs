//! `src/capture.rs` が提供する `AudioCapture` の公開 API に対する単体テスト。

// デバイス ID の C 文字列変換は FFI バックエンドでのみ行われるため、
// その挙動を検証するテストは FFI バックエンドが有効な場合だけコンパイルする。
#![cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]

use shiguredo_audio_device::{AudioCapture, AudioCaptureConfig, Error, Result};

/// 有効な FFI バックエンドを 1 つ選んでキャプチャを構築する
fn new_capture(config: AudioCaptureConfig) -> Result<AudioCapture> {
    #[cfg(enable_coreaudio)]
    {
        AudioCapture::new_coreaudio(config, |_| {})
    }
    #[cfg(all(not(enable_coreaudio), enable_pulse))]
    {
        AudioCapture::new_pulse(config, |_| {})
    }
    #[cfg(all(not(enable_coreaudio), not(enable_pulse), enable_pipewire))]
    {
        AudioCapture::new_pipewire(config, |_| {})
    }
}

/// NUL バイトを含むデバイス ID は C 側へ渡す前に拒否されること
#[test]
fn rejects_device_id_containing_nul() {
    let config = AudioCaptureConfig {
        device_id: Some("device\0id".to_string()),
        sample_rate: 48000,
        channels: 1,
    };

    let result = new_capture(config);

    assert!(
        matches!(result, Err(Error::InvalidDeviceId)),
        "NUL バイトを含むデバイス ID は InvalidDeviceId になること"
    );
}
