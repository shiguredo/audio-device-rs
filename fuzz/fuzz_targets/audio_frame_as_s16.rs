#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    data: Vec<u8>,
    frames: i32,
    channels: i32,
    sample_rate: i32,
    format: u8,
    timestamp_us: i64,
}

fuzz_target!(|input: Input| {
    let format = if input.format.is_multiple_of(2) {
        shiguredo_audio_device::AudioFormat::S16
    } else {
        shiguredo_audio_device::AudioFormat::F32
    };
    let frame = shiguredo_audio_device::AudioFrameOwned {
        data: input.data,
        frames: input.frames,
        channels: input.channels,
        sample_rate: input.sample_rate,
        format,
        timestamp_us: input.timestamp_us,
    };
    let _ = frame.as_s16();
    let _ = frame.as_f32();
});
