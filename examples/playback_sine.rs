//! 440Hz サイン波再生サンプル
//!
//! デフォルト出力デバイスに 440Hz のサイン波を S16 フォーマットで再生する。
//! Enter キーで停止する。

use std::f64::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use shiguredo_audio_device::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

const FREQUENCY: f64 = 440.0;
const AMPLITUDE: f64 = 0.5;

fn main() {
    // 実際のサンプルレート・チャンネル数は AudioPlayback 作成後に確定する。
    // クロージャから参照できるよう Arc<AtomicI32> で保持する。
    let sample_rate = Arc::new(AtomicI32::new(0));
    let channels = Arc::new(AtomicI32::new(0));
    let frame_position = Arc::new(AtomicU64::new(0));

    let sample_rate_clone = Arc::clone(&sample_rate);
    let channels_clone = Arc::clone(&channels);
    let frame_position_clone = Arc::clone(&frame_position);

    let mut playback = AudioPlayback::new(AudioPlaybackConfig::default(), move || {
        let sr = sample_rate_clone.load(Ordering::Relaxed);
        let ch = channels_clone.load(Ordering::Relaxed);
        if sr <= 0 || ch <= 0 {
            return None;
        }

        // 10ms 分のフレームを生成
        let frames_per_callback = sr / 100;
        let total_samples = (frames_per_callback * ch) as usize;
        let mut samples = vec![0i16; total_samples];

        let pos = frame_position_clone.load(Ordering::Relaxed);
        let sr_f64 = sr as f64;

        for i in 0..frames_per_callback as usize {
            let t = (pos as f64 + i as f64) / sr_f64;
            let value = (2.0 * PI * FREQUENCY * t).sin() * AMPLITUDE;
            let sample = (value * i16::MAX as f64) as i16;

            for c in 0..ch as usize {
                samples[i * ch as usize + c] = sample;
            }
        }

        frame_position_clone.fetch_add(frames_per_callback as u64, Ordering::Relaxed);

        PlaybackFrame::from_s16(&samples, ch, sr).ok()
    })
    .expect("failed to create playback session");

    sample_rate.store(playback.sample_rate(), Ordering::Relaxed);
    channels.store(playback.channels(), Ordering::Relaxed);

    eprintln!(
        "Playback started: {}Hz sine wave (sample rate: {}Hz, channels: {})",
        FREQUENCY,
        playback.sample_rate(),
        playback.channels()
    );
    eprintln!("Press Enter to stop");

    playback.start().expect("failed to start playback");

    // Enter キーの入力を待機
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);

    playback.stop();
    eprintln!("Playback stopped");
}
