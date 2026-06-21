#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() >= 2 {
        let (_prefix, aligned, _suffix) = unsafe { data.align_to::<i16>() };
        let _ = shiguredo_audio_device::PlaybackFrame::from_s16(aligned, 1, 48000);
    }
    if data.len() >= 4 {
        let (_prefix, aligned, _suffix) = unsafe { data.align_to::<f32>() };
        let _ = shiguredo_audio_device::PlaybackFrame::from_f32(aligned, 1, 48000);
    }
});
