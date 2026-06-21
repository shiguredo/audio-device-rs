#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: (Vec<f32>, i32, i32)| {
    let (data, channels, sample_rate) = data;
    let _ = shiguredo_audio_device::PlaybackFrame::from_f32(&data, channels, sample_rate);
});
