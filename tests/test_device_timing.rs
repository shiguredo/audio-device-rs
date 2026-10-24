//! 実機の入力デバイスと出力デバイスを使ったキャプチャ・再生タイミングの検証。
//!
//! デフォルトバックエンドで列挙したすべての入力デバイスと出力デバイスを対象にする。
//! デバイスが無い環境や列挙に失敗した環境では、その方向の検証をスキップして成功扱いにする。
//! バックエンドはデフォルト feature で選ばれるものだけを使う。

#![cfg(any(
    enable_default_coreaudio,
    enable_default_pulse,
    enable_default_pipewire,
    enable_default_wasapi
))]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use shiguredo_audio_device::{
    AudioCapture, AudioCaptureConfig, AudioDeviceList, AudioFrameOwned, AudioPlayback,
    AudioPlaybackConfig, PlaybackFrame, initialize_thread,
};

/// 各入力デバイスでキャプチャを続ける時間。
const CAPTURE_DURATION: Duration = Duration::from_secs(2);

/// 各出力デバイスで再生を続ける時間。
const PLAYBACK_DURATION: Duration = Duration::from_secs(3);

/// 再生開始直後の過渡状態を測定から除外する時間。
const PLAYBACK_WARMUP: Duration = Duration::from_secs(1);

/// 連続するキャプチャフレームのタイムスタンプ差に許す誤差 (マイクロ秒)。
const CAPTURE_TIMESTAMP_TOLERANCE_US: i64 = 1_000;

/// 再生コールバックの要求フレーム数と経過時間に許す誤差。
///
/// コールバックの周期や OS のスケジューリングによるずれを許容する。
const PLAYBACK_TIMING_TOLERANCE: Duration = Duration::from_millis(250);

/// 再生するサイン波の周波数 (Hz)。
const SIGNAL_FREQUENCY_HZ: f64 = 440.0;

/// 再生するサイン波の振幅 (フルスケールに対する比率)。
const SIGNAL_AMPLITUDE: f64 = 0.1;

/// 再生コールバック 1 回分の記録。
#[derive(Debug, Clone, Copy)]
struct PlaybackCall {
    /// コールバックが呼ばれた時刻。
    at: Instant,
    /// 要求されたフレーム数。
    frames: i32,
    /// 要求されたチャンネル数。
    channels: i32,
    /// 要求されたサンプルレート。
    sample_rate: i32,
}

/// 入力デバイスを列挙する。
///
/// デバイスが 1 件も無い場合は、検証をスキップする理由を出力して `None` を返す。
/// 列挙に失敗した場合はテストを失敗させる。
fn enumerate_input_devices() -> Option<AudioDeviceList> {
    let devices = AudioDeviceList::enumerate_input()
        .unwrap_or_else(|error| panic!("入力デバイスの列挙に失敗しました: {error}"));
    if devices.is_empty() {
        eprintln!("入力デバイスが無いためキャプチャのタイミング検証をスキップします");
        return None;
    }
    Some(devices)
}

/// 出力デバイスを列挙する。
///
/// デバイスが 1 件も無い場合は、検証をスキップする理由を出力して `None` を返す。
/// 列挙に失敗した場合はテストを失敗させる。
fn enumerate_output_devices() -> Option<AudioDeviceList> {
    let devices = AudioDeviceList::enumerate_output()
        .unwrap_or_else(|error| panic!("出力デバイスの列挙に失敗しました: {error}"));
    if devices.is_empty() {
        eprintln!("出力デバイスが無いため再生のタイミング検証をスキップします");
        return None;
    }
    Some(devices)
}

/// 入力デバイスごとに、キャプチャしたフレームのタイムスタンプが音声時間と一致すること。
#[test]
fn capture_timestamps_track_audio_time() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");

    let Some(device_list) = enumerate_input_devices() else {
        return;
    };

    for device in device_list.as_slice() {
        let name = device
            .name()
            .unwrap_or_else(|error| panic!("入力デバイス名の取得に失敗しました: {error}"));
        let unique_id = device.unique_id().unwrap_or_else(|error| {
            panic!("入力デバイス {name} の一意識別子の取得に失敗しました: {error}")
        });
        eprintln!("入力デバイス {name} のキャプチャタイミングを検証します");

        // コールバックからテストスレッドへフレームを渡すため、チャネルを使う
        let (sender, receiver) = mpsc::channel();
        let config = AudioCaptureConfig {
            device_id: Some(unique_id),
            sample_rate: device.sample_rate(),
            channels: device.channels(),
        };
        let mut capture = AudioCapture::new(config, move |frame| {
            // 受信側が先に終了しても検証結果には影響しないため、送信エラーは無視する
            let _ = sender.send(frame.to_owned());
        })
        .unwrap_or_else(|error| {
            panic!("入力デバイス {name} のキャプチャを作成できません: {error}")
        });

        capture.start().unwrap_or_else(|error| {
            panic!("入力デバイス {name} のキャプチャを開始できません: {error}")
        });
        std::thread::sleep(CAPTURE_DURATION);
        capture.stop();
        // キャプチャ停止後に受信を打ち切る
        drop(capture);

        let frames: Vec<AudioFrameOwned> = receiver.try_iter().collect();
        assert!(
            !frames.is_empty(),
            "入力デバイス {name} からフレームが 1 つも届きませんでした"
        );
        verify_capture_timestamps(&name, &frames);
    }
}

/// 出力デバイスごとに、再生コールバックが呼ばれて要求フレーム数が経過時間に追従すること。
#[test]
fn playback_callbacks_track_audio_time() {
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");

    let Some(device_list) = enumerate_output_devices() else {
        return;
    };

    for device in device_list.as_slice() {
        let name = device
            .name()
            .unwrap_or_else(|error| panic!("出力デバイス名の取得に失敗しました: {error}"));
        let unique_id = device.unique_id().unwrap_or_else(|error| {
            panic!("出力デバイス {name} の一意識別子の取得に失敗しました: {error}")
        });
        eprintln!("出力デバイス {name} の再生タイミングを検証します");

        // コールバックからテストスレッドへ呼び出し時刻と要求内容を渡すため、
        // チャネルを使う
        let (sender, receiver) = mpsc::channel();
        // コールバックをまたいでサイン波の位相を継続させるための生成済みフレーム数
        let phase_frames = Arc::new(AtomicU64::new(0));
        let callback_phase_frames = Arc::clone(&phase_frames);
        let mut playback = AudioPlayback::new(
            AudioPlaybackConfig {
                device_id: Some(unique_id),
                sample_rate: device.sample_rate(),
                channels: device.channels(),
            },
            move |frames, channels, sample_rate| {
                let _ = sender.send(PlaybackCall {
                    at: Instant::now(),
                    frames,
                    channels,
                    sample_rate,
                });
                generate_signal(frames, channels, sample_rate, &callback_phase_frames)
            },
        )
        .unwrap_or_else(|error| panic!("出力デバイス {name} の再生を作成できません: {error}"));

        playback
            .start()
            .unwrap_or_else(|error| panic!("出力デバイス {name} の再生を開始できません: {error}"));
        std::thread::sleep(PLAYBACK_DURATION);
        playback.stop();
        // 再生停止後に受信を打ち切る
        drop(playback);

        let calls: Vec<PlaybackCall> = receiver.try_iter().collect();
        verify_playback_calls(&name, &calls);
    }
}

/// 連続するキャプチャフレームのタイムスタンプが単調に増加し、直前のフレームの
/// 音声時間と 1 ms 以内で一致することを検証する。
///
/// タイムスタンプの差が直前のフレームの音声時間より 1 ms を超えて大きい組は、
/// その間にフレームが届かなかった区間とみなして検証から除外する。
fn verify_capture_timestamps(device_name: &str, frames: &[AudioFrameOwned]) {
    let mut verified_pairs = 0usize;
    let mut skipped_pairs = 0usize;

    for pair in frames.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];

        assert!(
            previous.frames > 0 && previous.sample_rate > 0,
            "入力デバイス {device_name} のフレームが不正です: frames={}, sample_rate={}",
            previous.frames,
            previous.sample_rate
        );
        assert!(
            current.frames > 0 && current.sample_rate > 0,
            "入力デバイス {device_name} のフレームが不正です: frames={}, sample_rate={}",
            current.frames,
            current.sample_rate
        );

        let timestamp_diff_us = current
            .timestamp_us
            .checked_sub(previous.timestamp_us)
            .unwrap_or_else(|| {
                panic!("入力デバイス {device_name} のタイムスタンプ差が i64 の範囲を超えました")
            });
        assert!(
            timestamp_diff_us > 0,
            "入力デバイス {device_name} のタイムスタンプが単調増加していません: {} -> {}",
            previous.timestamp_us,
            current.timestamp_us
        );

        // 直前のフレームの音声時間をマイクロ秒に換算する
        let audio_duration_us = previous.frames as i64 * 1_000_000 / previous.sample_rate as i64;
        let difference_us = timestamp_diff_us - audio_duration_us;

        if difference_us > CAPTURE_TIMESTAMP_TOLERANCE_US {
            // 音声時間よりタイムスタンプが大きく進んでいる場合は、その間に
            // フレームが届かなかった区間とみなす
            skipped_pairs += 1;
            continue;
        }

        assert!(
            difference_us >= -CAPTURE_TIMESTAMP_TOLERANCE_US,
            "入力デバイス {device_name} のタイムスタンプ差 {timestamp_diff_us} us が直前のフレームの音声時間 {audio_duration_us} us より 1 ms を超えて小さいです"
        );
        verified_pairs += 1;
    }

    assert!(
        verified_pairs > 0,
        "入力デバイス {device_name} でタイムスタンプを検証できるフレームの組がありません: {} フレーム, {} 組を欠落として除外",
        frames.len(),
        skipped_pairs
    );

    eprintln!(
        "入力デバイス {device_name}: {verified_pairs} 組のタイムスタンプを検証し、{skipped_pairs} 組を欠落として除外しました"
    );
}

/// 再生コールバックの回数と要求フレーム数が、経過時間に追従していることを検証する。
fn verify_playback_calls(device_name: &str, calls: &[PlaybackCall]) {
    assert!(
        !calls.is_empty(),
        "出力デバイス {device_name} の再生コールバックが 1 回も呼ばれませんでした"
    );

    let first = calls[0];
    assert!(
        first.frames > 0 && first.channels > 0 && first.sample_rate > 0,
        "出力デバイス {device_name} のコールバック引数が不正です: frames={}, channels={}, sample_rate={}",
        first.frames,
        first.channels,
        first.sample_rate
    );

    let mut cumulative_frames = 0i64;
    for call in calls {
        assert!(
            call.frames > 0,
            "出力デバイス {device_name} のコールバックで要求フレーム数が 0 以下になりました: {}",
            call.frames
        );
        assert_eq!(
            call.channels, first.channels,
            "出力デバイス {device_name} のコールバックでチャンネル数が変化しました"
        );
        assert_eq!(
            call.sample_rate, first.sample_rate,
            "出力デバイス {device_name} のコールバックでサンプルレートが変化しました"
        );
        cumulative_frames += call.frames as i64;
    }
    assert!(
        cumulative_frames > 0,
        "出力デバイス {device_name} の要求フレーム数の累積が増えませんでした"
    );

    // 再生開始直後の過渡状態を除外するため、最初のコールバックから一定時間経過した
    // 後のコールバックだけを対象にする。定常状態では要求フレーム数の増分が経過時間と
    // 一致する。
    let measurement_start = first.at + PLAYBACK_WARMUP;
    let measured_calls: Vec<&PlaybackCall> = calls
        .iter()
        .filter(|call| call.at >= measurement_start)
        .collect();
    assert!(
        measured_calls.len() >= 2,
        "出力デバイス {device_name} の測定区間に再生コールバックが 2 回以上ありません: {} 回",
        measured_calls.len()
    );

    let start = measured_calls
        .first()
        .expect("測定区間の先頭のコールバックがあること");
    let end = measured_calls
        .last()
        .expect("測定区間の末尾のコールバックがあること");
    let elapsed = end.at.duration_since(start.at);
    assert!(
        elapsed > Duration::ZERO,
        "出力デバイス {device_name} の測定区間の経過時間が 0 です"
    );

    // 測定区間の末尾で要求したフレームは、その時点ではまだ再生が完了していないため、
    // 経過時間と比較するフレーム数には含めない。これにより、測定区間中に再生された
    // フレーム数と経過時間を比較できる。
    let requested_frames: i64 = measured_calls
        .iter()
        .take(measured_calls.len() - 1)
        .map(|call| call.frames as i64)
        .sum();
    let expected_frames = elapsed.as_secs_f64() * first.sample_rate as f64;
    let tolerance_frames = PLAYBACK_TIMING_TOLERANCE.as_secs_f64() * first.sample_rate as f64;
    let difference_frames = requested_frames as f64 - expected_frames;

    assert!(
        difference_frames.abs() <= tolerance_frames,
        "出力デバイス {device_name} の要求フレーム数が経過時間に追従していません: 経過 {:.3} 秒, 要求 {} フレーム, 期待 {:.0} フレーム, 差 {:.0} フレーム, 許容 {:.0} フレーム",
        elapsed.as_secs_f64(),
        requested_frames,
        expected_frames,
        difference_frames,
        tolerance_frames
    );

    eprintln!(
        "出力デバイス {device_name}: {} 回のコールバックで {} フレームを要求し、{:.3} 秒の経過に対する期待値との差は {:.0} フレームでした",
        calls.len(),
        cumulative_frames,
        elapsed.as_secs_f64(),
        difference_frames
    );
}

/// 再生用の 440 Hz サイン波を生成する。
///
/// 位相をコールバック間で継続させるために、`phase_frames` に生成済みのフレーム数を
/// 記録する。`PlaybackFrame` を作成できない場合は `None` を返す。
/// `None` は無音として扱われる。
fn generate_signal(
    frames: i32,
    channels: i32,
    sample_rate: i32,
    phase_frames: &AtomicU64,
) -> Option<PlaybackFrame> {
    if frames <= 0 || channels <= 0 || sample_rate <= 0 {
        return None;
    }

    // このコールバックが担当する区間の先頭フレーム番号を確保する
    let start_frame = phase_frames.fetch_add(frames as u64, Ordering::Relaxed);
    let mut samples = Vec::new();
    for frame in 0..frames as u64 {
        let index = start_frame + frame;
        let phase = std::f64::consts::TAU * SIGNAL_FREQUENCY_HZ * index as f64 / sample_rate as f64;
        let sample = (phase.sin() * f64::from(i16::MAX) * SIGNAL_AMPLITUDE) as i16;
        for _ in 0..channels {
            samples.push(sample);
        }
    }

    PlaybackFrame::from_s16(&samples, channels, sample_rate).ok()
}
