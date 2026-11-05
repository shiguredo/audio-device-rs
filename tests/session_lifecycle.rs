//! デフォルトバックエンドのセッション開始・停止・再開始を検証する。
//!
//! 実行中に別スレッドから停止するケースと、ガードの Drop で停止するケースも検証する。
//! 入出力デバイスが 0 件の場合は検証をスキップする。
//! デバイスの列挙に失敗した場合はテストを失敗させる。
//! PipeWire などサーバを必要とするバックエンドでは、サーバが動作している必要がある。
//! サーバが停止している場合、デバイス列挙に失敗してテストは失敗する。
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

/// シナリオ全体の完了を待つ上限時間。
const SCENARIO_TIMEOUT: Duration = Duration::from_secs(60);

/// 1 つのテストで start / stop を繰り返す回数。
const LIFECYCLE_CYCLES: usize = 10;

/// 停止後に新しい process コールバックが届かないことを確認する時間。
const STOP_OBSERVE_TIMEOUT: Duration = Duration::from_millis(200);

/// デフォルトバックエンドの入力デバイス数を返す。
///
/// 列挙に失敗した場合は panic する。
fn input_device_count() -> usize {
    AudioDeviceList::enumerate_input()
        .unwrap_or_else(|error| {
            panic!("入力デバイスの列挙に失敗しました (オーディオサーバが動作しているか確認してください): {error}")
        })
        .len()
}

/// デフォルトバックエンドの出力デバイス数を返す。
///
/// 列挙に失敗した場合は panic する。
fn output_device_count() -> usize {
    AudioDeviceList::enumerate_output()
        .unwrap_or_else(|error| {
            panic!("出力デバイスの列挙に失敗しました (オーディオサーバが動作しているか確認してください): {error}")
        })
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

/// 停止後に新しい process コールバックが届かないことを確認する。
///
/// 停止処理中に送信済みの通知は捨て、その後一定時間新しい通知が届かないことを
/// 確認する。停止が実際に効いていなければ、この待機中に通知が届く。
fn assert_no_process_after_stop(receiver: &mpsc::Receiver<()>) {
    // 停止処理中に送信された分を捨てる
    while receiver.try_recv().is_ok() {}
    std::thread::sleep(STOP_OBSERVE_TIMEOUT);
    assert!(
        receiver.try_recv().is_err(),
        "停止後も process コールバックが届いています"
    );
}

/// コールバックをブロックしたまま、別スレッドから stop を呼ぶ。
///
/// stop がコールバックの完了を待つことを確認する。
fn stop_while_callback_blocked<'a, S, F>(release: mpsc::Receiver<()>, session: S, stop: F)
where
    S: Send + 'a,
    F: FnOnce(S) + Send + 'a,
{
    let (stop_started_sender, stop_started_receiver) = mpsc::channel();
    std::thread::scope(move |scope| {
        // アサーションが失敗しても release の drop でコールバックを解放する
        let handle = scope.spawn(move || {
            let _guard = initialize_thread().expect("停止スレッドを初期化できること");
            let _ = stop_started_sender.send(());
            stop(session);
        });

        stop_started_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("停止スレッドが停止処理を開始すること");
        // コールバックを止めている間は stop が戻らないことを確認する
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !handle.is_finished(),
            "コールバック実行中の stop が先に戻っています"
        );

        // コールバックの release.send を完了させる
        release
            .recv_timeout(Duration::from_secs(5))
            .expect("コールバックを解放できること");
    });
}

/// ワーカースレッドでセッションのシナリオを実行し、指定時間内に戻ることを検証する。
///
/// シナリオが指定時間内に戻らない場合は panic する。
fn run_with_watchdog<F>(label: &str, scenario: F)
where
    F: FnOnce() -> Result<(), Error> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    // シナリオがハングしてもテストプロセス全体が停止しないよう、
    // ワーカースレッドで実行してタイムアウトで監視する
    std::thread::spawn(move || {
        let _guard = initialize_thread().expect("ワーカースレッドを初期化できること");
        let _ = sender.send(scenario());
    });

    match receiver.recv_timeout(SCENARIO_TIMEOUT) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("{label}に失敗しました: {error}"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "{} 秒以内に{label}が戻りませんでした",
                SCENARIO_TIMEOUT.as_secs()
            )
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{label}を実行したワーカースレッドが終了しました")
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

    run_with_watchdog("キャプチャ", || {
        let (sender, receiver) = mpsc::channel();
        let mut capture = AudioCapture::new(AudioCaptureConfig::default(), move |_| {
            let _ = sender.send(());
        })?;

        for cycle in 0..LIFECYCLE_CYCLES {
            let session = capture.start()?;
            wait_for_process(&receiver);
            if cycle % 2 == 0 {
                session.stop();
            } else {
                // ガードの Drop でも停止する
                drop(session);
            }
        }
        // 最後のサイクルはガードの Drop で停止しているため、
        // 停止後にコールバックが届かないことを確認する
        assert_no_process_after_stop(&receiver);
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

    run_with_watchdog("再生", || {
        let (sender, receiver) = mpsc::channel();
        let mut playback = AudioPlayback::new(AudioPlaybackConfig::default(), move |_, _, _| {
            let _ = sender.send(());
            None
        })?;

        for cycle in 0..LIFECYCLE_CYCLES {
            let session = playback.start()?;
            wait_for_process(&receiver);
            if cycle % 2 == 0 {
                session.stop();
            } else {
                // ガードの Drop でも停止する
                drop(session);
            }
        }
        // 最後のサイクルはガードの Drop で停止しているため、
        // 停止後にコールバックが届かないことを確認する
        assert_no_process_after_stop(&receiver);
        Ok(())
    });
}

/// コールバック実行中に、別スレッドからガード経由で停止できること。
///
/// コールバックをブロックしたまま停止スレッドから stop を呼び、stop が
/// コールバックの終了を待つことを確認する。
#[test]
fn capture_stop_from_other_thread() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");

    if skip_capture() {
        return;
    }

    run_with_watchdog("キャプチャの別スレッド停止", || {
        let (sender, receiver) = mpsc::sync_channel(0);
        let mut capture = AudioCapture::new(AudioCaptureConfig::default(), move |_| {
            // 1 回目で開始を通知し、2 回目でテスト側の確認が終わるまで待つ
            let _ = sender.send(());
            let _ = sender.send(());
        })?;

        let session = capture.start()?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("process コールバックが届くこと");

        // ガードを停止用のスレッドへ move し、そのスレッドから停止する
        stop_while_callback_blocked(receiver, session, |session| session.stop());

        Ok(())
    });
}

/// コールバック実行中に、別スレッドからガード経由で停止できること。
///
/// コールバックをブロックしたまま停止スレッドから stop を呼び、stop が
/// コールバックの終了を待つことを確認する。
#[test]
fn playback_stop_from_other_thread() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");

    if skip_playback() {
        return;
    }

    run_with_watchdog("再生の別スレッド停止", || {
        let (sender, receiver) = mpsc::sync_channel(0);
        let mut playback = AudioPlayback::new(AudioPlaybackConfig::default(), move |_, _, _| {
            // 1 回目で開始を通知し、2 回目でテスト側の確認が終わるまで待つ
            let _ = sender.send(());
            let _ = sender.send(());
            None
        })?;

        let session = playback.start()?;
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("process コールバックが届くこと");

        // ガードを停止用のスレッドへ move し、そのスレッドから stop を呼ぶ
        stop_while_callback_blocked(receiver, session, |session| session.stop());

        Ok(())
    });
}
