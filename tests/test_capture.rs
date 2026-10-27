//! `src/capture.rs` が提供する `AudioCapture` の公開 API に対する単体テスト。

// デバイス ID の C 文字列変換は FFI バックエンドでのみ行われるため、
// その挙動を検証するテストは FFI バックエンドが有効な場合だけコンパイルする。
// コールバック境界のテストも FFI バックエンドの API を使うため、同じ条件で
// コンパイルする。
#![cfg(any(enable_coreaudio, enable_pulse, enable_pipewire))]

use std::cell::Cell;

use shiguredo_audio_device::{AudioCapture, AudioCaptureConfig, AudioFrame, Error, Result};

/// 有効な FFI バックエンドを 1 つ選んでキャプチャを構築する
fn new_capture<F>(config: AudioCaptureConfig, callback: F) -> Result<AudioCapture>
where
    F: Fn(AudioFrame<'_>) + Send + 'static,
{
    #[cfg(enable_coreaudio)]
    {
        AudioCapture::new_coreaudio(config, callback)
    }
    #[cfg(all(not(enable_coreaudio), enable_pulse))]
    {
        AudioCapture::new_pulse(config, callback)
    }
    #[cfg(all(not(enable_coreaudio), not(enable_pulse), enable_pipewire))]
    {
        AudioCapture::new_pipewire(config, callback)
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

    let result = new_capture(config, |_| {});

    assert!(
        matches!(result, Err(Error::InvalidDeviceId)),
        "NUL バイトを含むデバイス ID は InvalidDeviceId になること"
    );
}

/// `Cell<u64>` を捕捉したコールバックを渡せること。
///
/// このテストの目的は実行時の動作ではなく、コールバック内でだけ使う状態を
/// 捕捉したコールバックを渡せることのコンパイル確認である。デバイスが無い
/// 環境では `new` がエラーを返すため、`Ok` / `Err` のどちらでもテストを
/// 成功させる。
#[test]
fn accepts_callback_capturing_cell() {
    let calls = Cell::new(0u64);

    let result = new_capture(AudioCaptureConfig::default(), move |_| {
        calls.set(calls.get() + 1);
    });

    // 境界のコンパイル可否だけを確認するため、結果の種別は問わない
    let _ = result;
}
