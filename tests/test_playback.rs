//! `src/playback.rs` が提供する `AudioPlayback` の公開 API に対する単体テスト。

// 各テストはデフォルトバックエンドの API を使うため、デフォルトバックエンドが
// 有効な場合だけコンパイルする。
#![cfg(any(
    enable_default_coreaudio,
    enable_default_pulse,
    enable_default_pipewire,
    enable_default_wasapi
))]

use std::cell::Cell;

use shiguredo_audio_device::{AudioPlayback, AudioPlaybackConfig, Error, initialize_thread};

/// NUL バイトを含むデバイス ID は OS へ渡す前に拒否されること
#[test]
fn rejects_device_id_containing_nul() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");
    let config = AudioPlaybackConfig {
        device_id: Some("device\0id".to_string()),
        sample_rate: 48000,
        channels: 2,
    };

    let result = AudioPlayback::new(config, |_, _, _| None);

    assert!(
        matches!(result, Err(Error::InvalidDeviceId)),
        "NUL バイトを含むデバイス ID は InvalidDeviceId になること"
    );
}

/// デフォルトバックエンドの再生が `Cell<u64>` を捕捉したコールバックを
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
    let _ = AudioPlayback::new(AudioPlaybackConfig::default(), move |_, _, _| {
        calls.set(calls.get() + 1);
        None
    });
}
