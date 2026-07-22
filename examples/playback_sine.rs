//! 440Hz サイン波再生サンプル
//!
//! デフォルト出力デバイスに 440Hz のサイン波を S16 フォーマットで再生する。
//! Enter キーで停止する。

use std::f64::consts::PI;
use std::sync::atomic::{AtomicU64, Ordering};

use shiguredo_audio_device::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

const FREQUENCY: f64 = 440.0;
const AMPLITUDE: f64 = 0.5;

fn main() {
    let frame_position = AtomicU64::new(0);

    let mut playback = AudioPlayback::new(
        AudioPlaybackConfig::default(),
        move |requested_frames, channels, sample_rate| {
            let total = (requested_frames * channels) as usize;
            let mut samples = vec![0i16; total];

            let pos = frame_position.load(Ordering::Relaxed);
            let sr_f64 = sample_rate as f64;

            for i in 0..requested_frames as usize {
                let t = (pos as f64 + i as f64) / sr_f64;
                let value = (2.0 * PI * FREQUENCY * t).sin() * AMPLITUDE;
                let sample = (value * i16::MAX as f64) as i16;
                for c in 0..channels as usize {
                    samples[i * channels as usize + c] = sample;
                }
            }

            let frame = PlaybackFrame::from_s16(&samples, channels, sample_rate)
                .expect("PlaybackFrame::from_s16 must not fail with valid params");

            frame_position.store(pos + requested_frames as u64, Ordering::Relaxed);
            Some(frame)
        },
    )
    .expect("failed to create playback session");

    eprintln!(
        "Playback started: {}Hz sine wave (sample rate: {}Hz, channels: {})",
        FREQUENCY,
        playback.sample_rate(),
        playback.channels()
    );
    eprintln!("Press Enter to stop");

    playback.start().expect("failed to start playback");

    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);

    playback.stop();
    eprintln!("Playback stopped");
}
