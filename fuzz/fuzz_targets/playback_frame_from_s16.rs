#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: (Vec<i16>, i32, i32)| {
    let (data, channels, sample_rate) = data;
    let _ = shiguredo_audio_device::PlaybackFrame::from_s16(&data, channels, sample_rate);
});
