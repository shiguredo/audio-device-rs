//! `src/playback.rs` が提供する `AudioPlayback` の公開 API に対する単体テスト。

// デバイス ID の C 文字列変換は FFI バックエンドでのみ行われるため、
// その挙動を検証するテストは FFI バックエンドが有効な場合だけコンパイルする。
#![cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]

use shiguredo_audio_device::{AudioPlayback, AudioPlaybackConfig, Error, Result};

/// 有効な FFI バックエンドを 1 つ選んで再生を構築する
fn new_playback(config: AudioPlaybackConfig) -> Result<AudioPlayback> {
    #[cfg(enable_coreaudio)]
    {
        AudioPlayback::new_coreaudio(config, |_, _, _| None)
    }
    #[cfg(all(not(enable_coreaudio), enable_pulse))]
    {
        AudioPlayback::new_pulse(config, |_, _, _| None)
    }
    #[cfg(all(not(enable_coreaudio), not(enable_pulse), enable_pipewire))]
    {
        AudioPlayback::new_pipewire(config, |_, _, _| None)
    }
}

/// NUL バイトを含むデバイス ID は C 側へ渡す前に拒否されること
#[test]
fn rejects_device_id_containing_nul() {
    let config = AudioPlaybackConfig {
        device_id: Some("device\0id".to_string()),
        sample_rate: 48000,
        channels: 2,
    };

    let result = new_playback(config);

    assert!(
        matches!(result, Err(Error::InvalidDeviceId)),
        "NUL バイトを含むデバイス ID は InvalidDeviceId になること"
    );
}
