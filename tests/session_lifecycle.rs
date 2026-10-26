//! デフォルトバックエンドのセッション開始・停止・再開始を検証する。
//!
//! 入出力デバイスが 0 件の場合は検証をスキップする。
//! デバイスの列挙に失敗した場合はテストを失敗させる。
//! PipeWire などサーバを必要とするバックエンドでは、サーバが動作している必要がある。
//! macOS のキャプチャはマイクへのアクセス許可が必要なため検証しない。

#![cfg(any(
    enable_default_coreaudio,
    enable_default_pulse,
    enable_default_pipewire,
    enable_default_wasapi
))]

use std::sync::mpsc;
use std::time::Duration;

use shiguredo_audio_device::{
    AudioCapture, AudioCaptureConfig, AudioDeviceList, AudioPlayback, AudioPlaybackConfig, Error,
    initialize_thread,
};

/// セッション開始の完了を待つ上限時間。
const START_TIMEOUT: Duration = Duration::from_secs(20);

/// 1 つのテストで start / stop を繰り返す回数。
const LIFECYCLE_CYCLES: usize = 10;

/// デフォルトバックエンドの入力デバイス数を返す。
///
/// 列挙に失敗した場合は panic する。
fn input_device_count() -> usize {
    AudioDeviceList::enumerate_input()
        .unwrap_or_else(|error| panic!("入力デバイスの列挙に失敗しました: {error}"))
        .len()
}

/// デフォルトバックエンドの出力デバイス数を返す。
///
/// 列挙に失敗した場合は panic する。
fn output_device_count() -> usize {
    AudioDeviceList::enumerate_output()
        .unwrap_or_else(|error| panic!("出力デバイスの列挙に失敗しました: {error}"))
        .len()
}

/// キャプチャの検証をスキップするかどうかを返す。
///
/// macOS のキャプチャと、入力デバイスが 0 件の場合は検証しない。
/// デバイスの列挙に失敗した場合は panic する。
fn skip_capture() -> bool {
    if cfg!(enable_default_coreaudio) {
        eprintln!("macOS のキャプチャはマイクへのアクセス許可が必要なため検証をスキップします");
        return true;
    }
    if input_device_count() == 0 {
        eprintln!("入力デバイスが 0 件のためキャプチャの検証をスキップします");
        return true;
    }
    false
}

/// 再生の検証をスキップするかどうかを返す。
///
/// 出力デバイスが 0 件の場合は検証しない。デバイスの列挙に失敗した場合は panic する。
fn skip_playback() -> bool {
    if output_device_count() == 0 {
        eprintln!("出力デバイスが 0 件のため再生の検証をスキップします");
        return true;
    }
    false
}

/// セッションの process コールバックを待つ。
///
/// デバイスが PAUSED のままなど、コールバックが来ない場合もあるため、
/// タイムアウトしても続行する。
fn wait_for_process(receiver: &mpsc::Receiver<()>) {
    let _ = receiver.recv_timeout(Duration::from_secs(1));
}

/// ワーカースレッドでセッションを開始し、指定時間内に開始が戻ることを検証する。
///
/// 開始が指定時間内に戻らない場合は panic する。
fn run_start_with_watchdog<F>(label: &str, start: F)
where
    F: FnOnce() -> Result<(), Error> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    // 開始処理がハングしてもテストプロセス全体が停止しないよう、
    // ワーカースレッドで実行してタイムアウトで監視する
    std::thread::spawn(move || {
        let _guard = initialize_thread().expect("ワーカースレッドを初期化できること");
        let _ = sender.send(start());
    });

    match receiver.recv_timeout(START_TIMEOUT) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("{label}の開始に失敗しました: {error}"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "{} 秒以内に{label}の開始が戻りませんでした",
                START_TIMEOUT.as_secs()
            )
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{label}の開始を実行したワーカースレッドが終了しました")
        }
    }
}

/// 入力ストリームの開始と、stop 後の再開始がハング・クラッシュしないこと。
#[test]
fn capture_start_and_restart_after_stop() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");

    if skip_capture() {
        return;
    }

    run_start_with_watchdog("キャプチャ", || {
        let (sender, receiver) = mpsc::channel();
        let mut capture = AudioCapture::new(AudioCaptureConfig::default(), move |_| {
            let _ = sender.send(());
        })?;

        for _ in 0..LIFECYCLE_CYCLES {
            capture.start()?;
            wait_for_process(&receiver);
            capture.stop();
        }
        Ok(())
    });
}

/// 出力ストリームの開始と、stop 後の再開始がハング・クラッシュしないこと。
#[test]
fn playback_start_and_restart_after_stop() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");

    if skip_playback() {
        return;
    }

    run_start_with_watchdog("再生", || {
        let (sender, receiver) = mpsc::channel();
        let mut playback = AudioPlayback::new(AudioPlaybackConfig::default(), move |_, _, _| {
            let _ = sender.send(());
            None
        })?;

        for _ in 0..LIFECYCLE_CYCLES {
            playback.start()?;
            wait_for_process(&receiver);
            playback.stop();
        }
        Ok(())
    });
}
