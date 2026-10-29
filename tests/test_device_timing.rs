//! 実機の入力デバイスと出力デバイスを使ったキャプチャ・再生タイミングの検証。
//!
//! デフォルトバックエンドで選ばれるデフォルトの入力デバイスと出力デバイスを対象にする。
//! デバイスが無い環境では、その方向の検証をスキップして成功扱いにする。
//! デバイス列挙に失敗した場合はテストを失敗させる。
//! バックエンドはデフォルト feature で選ばれるものだけを使う。

#![cfg(any(
    enable_default_coreaudio,
    enable_default_pulse,
    enable_default_pipewire,
    enable_default_wasapi
))]

use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use shiguredo_audio_device::{
    AudioCapture, AudioCaptureConfig, AudioDeviceList, AudioFrameOwned, AudioPlayback,
    AudioPlaybackConfig, PlaybackFrame, initialize_thread,
};

/// デフォルト入力デバイスでキャプチャを続ける時間。
const CAPTURE_DURATION: Duration = Duration::from_secs(5);

/// デフォルト出力デバイスで再生を続ける時間。
const PLAYBACK_DURATION: Duration = Duration::from_secs(3);

/// 再生開始直後の過渡状態を測定から除外する時間。
const PLAYBACK_WARMUP: Duration = Duration::from_secs(1);

/// 連続するキャプチャフレームのタイムスタンプ差に許す誤差 (マイクロ秒)。
const CAPTURE_TIMESTAMP_TOLERANCE_US: i64 = 1_000;

/// タイムスタンプを検証するために必要な最低組数。
const MIN_VERIFIED_CAPTURE_PAIRS: usize = 2;

/// 再生コールバックのフレームレートに許す誤差。
///
/// 測定区間全体で見た要求フレーム数のずれが 50 ms 相当以内であることを表す。
/// コールバックの周期や OS のスケジューリングによるずれを許容する。
const PLAYBACK_TIMING_TOLERANCE: Duration = Duration::from_millis(50);

/// 実機デバイスを使うテストを直列に実行するためのロック。
///
/// 入出力を同時に動かすとコールバックのスレッドがスケジューリング遅延の影響を
/// 受けやすくなるため、テスト実行の排他制御にだけ使う。共有データの保護ではない。
static DEVICE_TEST_LOCK: Mutex<()> = Mutex::new(());

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

/// デフォルト入力デバイスで、キャプチャしたフレームのタイムスタンプが音声時間と一致すること。
#[test]
fn capture_timestamps_track_audio_time() {
    let _test_guard = DEVICE_TEST_LOCK
        .lock()
        .expect("デバイスを使うテストを直列化するロックを取得できること");
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");

    let Some(input_devices) = enumerate_input_devices() else {
        return;
    };
    drop(input_devices);

    eprintln!("デフォルト入力デバイスのキャプチャタイミングを検証します");

    // コールバックからテストスレッドへフレームを渡すため、チャネルを使う
    let (sender, receiver) = mpsc::channel();
    let mut capture = AudioCapture::new(AudioCaptureConfig::default(), move |frame| {
        // 受信側が先に終了しても検証結果には影響しないため、送信エラーは無視する
        let _ = sender.send(frame.to_owned());
    })
    .unwrap_or_else(|error| panic!("デフォルト入力デバイスのキャプチャを作成できません: {error}"));

    let session = capture.start().unwrap_or_else(|error| {
        panic!("デフォルト入力デバイスのキャプチャを開始できません: {error}")
    });
    std::thread::sleep(CAPTURE_DURATION);
    session.stop();
    // キャプチャ停止後に受信を打ち切る
    drop(capture);

    let frames: Vec<AudioFrameOwned> = receiver.try_iter().collect();
    assert!(
        !frames.is_empty(),
        "デフォルト入力デバイスからフレームが 1 つも届きませんでした"
    );
    verify_capture_timestamps("デフォルト入力デバイス", &frames);
}

/// デフォルト出力デバイスで、再生コールバックが呼ばれて要求フレーム数が経過時間に追従すること。
#[test]
fn playback_callbacks_track_audio_time() {
    let _test_guard = DEVICE_TEST_LOCK
        .lock()
        .expect("デバイスを使うテストを直列化するロックを取得できること");
    let _guard = initialize_thread().expect("テストスレッドを初期化できること");

    let Some(output_devices) = enumerate_output_devices() else {
        return;
    };
    drop(output_devices);

    eprintln!("デフォルト出力デバイスの再生タイミングを検証します");

    // コールバックからテストスレッドへ呼び出し時刻と要求内容を渡すため、
    // チャネルを使う
    let (sender, receiver) = mpsc::channel();
    // コールバックをまたいでサイン波の位相を継続させるための生成済みフレーム数
    let mut phase_frames = 0u64;
    let mut playback = AudioPlayback::new(
        AudioPlaybackConfig::default(),
        move |frames, channels, sample_rate| {
            let _ = sender.send(PlaybackCall {
                at: Instant::now(),
                frames,
                channels,
                sample_rate,
            });
            generate_signal(frames, channels, sample_rate, &mut phase_frames)
        },
    )
    .unwrap_or_else(|error| panic!("デフォルト出力デバイスの再生を作成できません: {error}"));

    let session = playback
        .start()
        .unwrap_or_else(|error| panic!("デフォルト出力デバイスの再生を開始できません: {error}"));
    std::thread::sleep(PLAYBACK_DURATION);
    session.stop();
    // 再生停止後に受信を打ち切る
    drop(playback);

    let calls: Vec<PlaybackCall> = receiver.try_iter().collect();
    verify_playback_calls("デフォルト出力デバイス", &calls);
}

/// 連続するキャプチャフレームのタイムスタンプが単調に増加し、直前のフレームの
/// 音声時間と 1 ms 以内で一致することを検証する。
///
/// 一致しない組が 1 つでもあれば失敗する。
fn verify_capture_timestamps(device_name: &str, frames: &[AudioFrameOwned]) {
    let mut verified_pairs = 0usize;

    for pair in frames.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];

        assert!(
            previous.frames > 0 && previous.sample_rate > 0,
            "{device_name} のフレームが不正です: frames={}, sample_rate={}",
            previous.frames,
            previous.sample_rate
        );
        assert!(
            current.frames > 0 && current.sample_rate > 0,
            "{device_name} のフレームが不正です: frames={}, sample_rate={}",
            current.frames,
            current.sample_rate
        );

        let timestamp_diff_us = current
            .timestamp_us
            .checked_sub(previous.timestamp_us)
            .unwrap_or_else(|| panic!("{device_name} のタイムスタンプ差が i64 の範囲を超えました"));
        assert!(
            timestamp_diff_us > 0,
            "{device_name} のタイムスタンプが単調増加していません: {} -> {}",
            previous.timestamp_us,
            current.timestamp_us
        );

        // 直前のフレームの音声時間をマイクロ秒に換算する
        let audio_duration_us = previous.frames as i64 * 1_000_000 / previous.sample_rate as i64;
        let difference_us = timestamp_diff_us - audio_duration_us;

        assert!(
            difference_us.abs() <= CAPTURE_TIMESTAMP_TOLERANCE_US,
            "{device_name} のタイムスタンプ差 {timestamp_diff_us} us が直前のフレームの音声時間 {audio_duration_us} us と 1 ms を超えてずれています"
        );
        verified_pairs += 1;
    }

    assert!(
        verified_pairs >= MIN_VERIFIED_CAPTURE_PAIRS,
        "{device_name} でタイムスタンプを検証できた組が {} 組しかありません (最低 {} 組): {} フレーム",
        verified_pairs,
        MIN_VERIFIED_CAPTURE_PAIRS,
        frames.len()
    );

    eprintln!("{device_name}: {verified_pairs} 組のタイムスタンプを検証しました");
}

/// 再生コールバックの要求フレーム数が、経過時間に比例した一定のレートで
/// 増えていることを検証する。
fn verify_playback_calls(device_name: &str, calls: &[PlaybackCall]) {
    assert!(
        !calls.is_empty(),
        "{device_name} の再生コールバックが 1 回も呼ばれませんでした"
    );

    let first = calls[0];
    assert!(
        first.frames > 0 && first.channels > 0 && first.sample_rate > 0,
        "{device_name} のコールバック引数が不正です: frames={}, channels={}, sample_rate={}",
        first.frames,
        first.channels,
        first.sample_rate
    );

    // 再生開始直後の過渡状態を除外するため、最初のコールバックから一定時間経過した
    // 後のコールバックだけを対象にする。定常状態では累積要求フレーム数が経過時間に
    // 対して一定の傾きで増える。
    let measurement_start = first.at + PLAYBACK_WARMUP;

    let mut cumulative_frames = 0i64;
    let mut points = Vec::new();
    for call in calls {
        assert!(
            call.frames > 0,
            "{device_name} のコールバックで要求フレーム数が 0 以下になりました: {}",
            call.frames
        );
        assert_eq!(
            call.channels, first.channels,
            "{device_name} のコールバックでチャンネル数が変化しました"
        );
        assert_eq!(
            call.sample_rate, first.sample_rate,
            "{device_name} のコールバックでサンプルレートが変化しました"
        );
        cumulative_frames += i64::from(call.frames);
        if call.at >= measurement_start {
            points.push(((call.at - first.at).as_secs_f64(), cumulative_frames as f64));
        }
    }
    assert!(
        cumulative_frames > 0,
        "{device_name} の要求フレーム数の累積が増えませんでした"
    );
    assert!(
        points.len() >= 2,
        "{device_name} の測定区間に再生コールバックが 2 回以上ありません: {} 回",
        points.len()
    );

    let (first_seconds, _) = points
        .first()
        .copied()
        .expect("測定区間の先頭の測定点があること");
    let (last_seconds, _) = points
        .last()
        .copied()
        .expect("測定区間の末尾の測定点があること");
    let elapsed = last_seconds - first_seconds;
    assert!(elapsed > 0.0, "{device_name} の測定区間の経過時間が 0 です");

    // 端点 2 点だけを使うと、測定区間の境界で 1 回遅延しただけで経過時間が
    // 変わってしまう。すべての測定点に最小二乗直線を当てはめ、1 回の遅延が
    // 推定レートに与える影響を測定点の数で薄める。
    let estimated_rate = estimate_frame_rate(&points)
        .expect("測定点が 2 点以上あり、時刻が異なるためレートを推定できること");

    let sample_rate = f64::from(first.sample_rate);
    let tolerance_frames = PLAYBACK_TIMING_TOLERANCE.as_secs_f64() * sample_rate;
    let difference_frames = (estimated_rate - sample_rate) * elapsed;

    assert!(
        difference_frames.abs() <= tolerance_frames,
        "{device_name} の再生フレームレートが経過時間に追従していません: 推定 {:.1} fps, 期待 {:.1} fps, 経過 {:.3} 秒, 差 {:.0} フレーム, 許容 {:.0} フレーム (測定点 {} 点)",
        estimated_rate,
        sample_rate,
        elapsed,
        difference_frames,
        tolerance_frames,
        points.len()
    );

    eprintln!(
        "{device_name}: {} 回のコールバックのうち {} 点で回帰し、推定レート {:.1} fps (期待 {:.1} fps)、{:.3} 秒の経過に対する差は {:.0} フレームでした",
        calls.len(),
        points.len(),
        estimated_rate,
        sample_rate,
        elapsed,
        difference_frames
    );
}

/// 測定点の最小二乗直線の傾きを求める。
///
/// `points` は (経過秒, 累積要求フレーム数) の組の列である。傾きをフレームレート
/// (フレーム/秒) として返す。2 点未満、または経過秒がすべて同じ場合は `None` を
/// 返す。
fn estimate_frame_rate(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }

    let count = points.len() as f64;
    let mean_seconds = points.iter().map(|point| point.0).sum::<f64>() / count;
    let mean_frames = points.iter().map(|point| point.1).sum::<f64>() / count;

    let mut covariance = 0.0;
    let mut variance = 0.0;
    for point in points {
        let seconds = point.0 - mean_seconds;
        covariance += seconds * (point.1 - mean_frames);
        variance += seconds * seconds;
    }

    // すべての測定点が同じ時刻なら直線の傾きを決められない
    if variance <= 0.0 {
        return None;
    }
    Some(covariance / variance)
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
    phase_frames: &mut u64,
) -> Option<PlaybackFrame> {
    if frames <= 0 || channels <= 0 || sample_rate <= 0 {
        return None;
    }

    // このコールバックが担当する区間の先頭フレーム番号を確保する
    let start_frame = *phase_frames;
    *phase_frames += frames as u64;
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

#[test]
fn estimate_frame_rate_ignores_single_delay() {
    // 10 ms ごとに呼ばれ、48 kHz で一定のフレーム数を要求する 2 秒間の合成データを
    // 作る。最後のコールバックだけが 70 ms 遅れた状況を再現する。このとき端点 2 点の
    // 経過時間は 70 ms 長くなるが、全点の回帰では傾きへの影響が薄まる。
    const SAMPLE_RATE: f64 = 48_000.0;
    const CALL_INTERVAL_SECONDS: f64 = 0.01;
    const CALL_COUNT: usize = 200;
    const DELAY_SECONDS: f64 = 0.07;

    let mut points = Vec::new();
    let mut frames = 0.0;
    for index in 0..CALL_COUNT {
        let delayed = if index + 1 == CALL_COUNT {
            DELAY_SECONDS
        } else {
            0.0
        };
        points.push((index as f64 * CALL_INTERVAL_SECONDS + delayed, frames));
        frames += SAMPLE_RATE * CALL_INTERVAL_SECONDS;
    }

    let estimated_rate = estimate_frame_rate(&points)
        .expect("測定点が 2 点以上あり、時刻が異なるためレートを推定できること");
    let elapsed = points.last().expect("測定点が 1 点以上あること").0
        - points.first().expect("測定点が 1 点以上あること").0;
    let difference_frames = (estimated_rate - SAMPLE_RATE) * elapsed;
    let tolerance_frames = PLAYBACK_TIMING_TOLERANCE.as_secs_f64() * SAMPLE_RATE;

    assert!(
        difference_frames.abs() <= tolerance_frames,
        "端点 1 点の 70 ms の遅延でレート推定が許容値を超えました: 推定 {estimated_rate:.1} fps, 差 {difference_frames:.0} フレーム, 許容 {tolerance_frames:.0} フレーム"
    );
}

#[test]
fn estimate_frame_rate_detects_double_rate() {
    // 10 ms ごとに 2 倍のフレーム数を要求する合成データを作る。回帰の傾きが
    // 2 倍のフレームレートになり、許容値を超えることを確認する。
    const SAMPLE_RATE: f64 = 48_000.0;
    const CALL_INTERVAL_SECONDS: f64 = 0.01;
    const CALL_COUNT: usize = 200;

    let mut points = Vec::new();
    let mut frames = 0.0;
    for index in 0..CALL_COUNT {
        points.push((index as f64 * CALL_INTERVAL_SECONDS, frames));
        frames += 2.0 * SAMPLE_RATE * CALL_INTERVAL_SECONDS;
    }

    let estimated_rate = estimate_frame_rate(&points)
        .expect("測定点が 2 点以上あり、時刻が異なるためレートを推定できること");
    let elapsed = points.last().expect("測定点が 1 点以上あること").0
        - points.first().expect("測定点が 1 点以上あること").0;
    let difference_frames = (estimated_rate - SAMPLE_RATE) * elapsed;
    let tolerance_frames = PLAYBACK_TIMING_TOLERANCE.as_secs_f64() * SAMPLE_RATE;

    assert!(
        difference_frames.abs() > tolerance_frames,
        "2 倍のレートの合成データが許容値に収まりました: 推定 {estimated_rate:.1} fps, 差 {difference_frames:.0} フレーム, 許容 {tolerance_frames:.0} フレーム"
    );
}

#[test]
fn estimate_frame_rate_returns_none_without_time_span() {
    // 傾きを決めるには、時刻の異なる 2 点以上が必要になる。
    assert_eq!(
        estimate_frame_rate(&[]),
        None,
        "測定点が無い場合は推定できないこと"
    );
    assert_eq!(
        estimate_frame_rate(&[(1.0, 0.0)]),
        None,
        "測定点が 1 点の場合は推定できないこと"
    );
    assert_eq!(
        estimate_frame_rate(&[(1.0, 0.0), (1.0, 480.0)]),
        None,
        "測定点の時刻がすべて同じ場合は推定できないこと"
    );
}
