//! 440Hz サイン波再生サンプル
//!
//! デフォルト出力デバイスに 440Hz のサイン波を S16 フォーマットで再生する。
//! Enter キーで停止する。

use std::f64::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use shiguredo_audio_device::{AudioPlayback, AudioPlaybackConfig, PlaybackFrame};

const FREQUENCY: f64 = 440.0;
const AMPLITUDE: f64 = 0.5;

fn main() {
    let config = AudioPlaybackConfig::default();
    let sample_rate = config.sample_rate;
    let channels = config.channels;

    let frame_position = Arc::new(AtomicU64::new(0));
    let frame_position_clone = Arc::clone(&frame_position);

    let mut playback = AudioPlayback::new(config, move || {
        // 10ms 分のフレームを生成
        let frames_per_callback = sample_rate / 100;
        let total_samples = (frames_per_callback * channels) as usize;
        let mut samples = vec![0i16; total_samples];

        let pos = frame_position_clone.load(Ordering::Relaxed);
        let sr = sample_rate as f64;

        for i in 0..frames_per_callback as usize {
            let t = (pos as f64 + i as f64) / sr;
            let value = (2.0 * PI * FREQUENCY * t).sin() * AMPLITUDE;
            let sample = (value * i16::MAX as f64) as i16;

            for ch in 0..channels as usize {
                samples[i * channels as usize + ch] = sample;
            }
        }

        frame_position_clone.fetch_add(frames_per_callback as u64, Ordering::Relaxed);

        Some(PlaybackFrame::from_s16(&samples, channels, sample_rate))
    })
    .expect("再生セッションの作成に失敗");

    eprintln!(
        "再生開始: {}Hz サイン波 (サンプルレート: {}Hz, チャンネル数: {})",
        FREQUENCY,
        playback.sample_rate(),
        playback.channels()
    );
    eprintln!("Enter キーで停止");

    playback.start().expect("再生の開始に失敗");

    // Enter キーの入力を待機
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);

    playback.stop();
    eprintln!("再生停止");
}
