//! `src/capture.rs` が提供する `AudioCapture` の公開 API に対する単体テスト。

// 各テストはデフォルトバックエンドの API を使うため、デフォルトバックエンドが
// 有効な場合だけコンパイルする。
#![cfg(any(
    enable_default_coreaudio,
    enable_default_pulse,
    enable_default_pipewire,
    enable_default_wasapi
))]

use std::cell::Cell;
use std::collections::VecDeque;

use shiguredo_audio_device::{AudioCapture, AudioCaptureConfig, Error, initialize_thread};

/// NUL バイトを含むデバイス ID は OS へ渡す前に拒否されること
#[test]
fn rejects_device_id_containing_nul() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");
    let config = AudioCaptureConfig {
        device_id: Some("device\0id".to_string()),
        sample_rate: 48000,
        channels: 1,
    };

    let result = AudioCapture::new(config, |_| {});

    assert!(
        matches!(result, Err(Error::InvalidDeviceId)),
        "NUL バイトを含むデバイス ID は InvalidDeviceId になること"
    );
}

/// デフォルトバックエンドのキャプチャが `Cell<u64>` を捕捉したコールバックを
/// 受け付けること。
///
/// このテストの目的は実行時の動作ではなく、コールバック内でだけ使う状態を
/// 捕捉したコールバックを渡せることのコンパイル確認である。デバイスが無い
/// 環境では `new` がエラーを返すため、`Ok` / `Err` のどちらでもテストを
/// 成功させる。
#[test]
fn accepts_callback_capturing_cell() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");
    let calls = Cell::new(0u64);

    // 境界のコンパイル可否だけを確認するため、結果の種別は問わない
    let _ = AudioCapture::new(AudioCaptureConfig::default(), move |_| {
        calls.set(calls.get() + 1);
    });
}

/// デフォルトバックエンドのキャプチャが、`FnMut` のコールバック内で直接更新する
/// 状態を捕捉したコールバックを受け付けること。
///
/// このテストの目的は実行時の動作ではなく、`Cell` / `RefCell` / `Atomic` などを
/// 使わずに `u64` や `VecDeque` をコールバック内で更新できることのコンパイル
/// 確認である。デバイスが無い環境では `new` がエラーを返すため、`Ok` / `Err` の
/// どちらでもテストを成功させる。
#[test]
fn accepts_callback_mutating_captured_state() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");
    let mut received_frames = 0u64;
    let mut recent_frames = VecDeque::new();

    // 境界のコンパイル可否だけを確認するため、結果の種別は問わない
    let _ = AudioCapture::new(AudioCaptureConfig::default(), move |frame| {
        received_frames += frame.frames as u64;
        recent_frames.push_back(received_frames);
    });
}
