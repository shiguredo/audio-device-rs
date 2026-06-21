use proptest::prelude::*;
use shiguredo_audio_device::{AudioFormat, AudioFrameOwned};

fn arb_valid_s16_frame() -> impl Strategy<Value = AudioFrameOwned> {
    (1i32..=128, 1i32..=8).prop_flat_map(|(frames, channels)| {
        let sample_count = (frames as usize) * (channels as usize);
        let byte_count = sample_count * 2;
        (
            proptest::collection::vec(any::<u8>(), byte_count..=byte_count),
            Just(frames),
            Just(channels),
        )
            .prop_map(|(data, frames, channels)| AudioFrameOwned {
                data,
                frames,
                channels,
                sample_rate: 48000,
                format: AudioFormat::S16,
                timestamp_us: 0,
            })
    })
}

fn arb_valid_f32_frame() -> impl Strategy<Value = AudioFrameOwned> {
    (1i32..=128, 1i32..=8).prop_flat_map(|(frames, channels)| {
        let sample_count = (frames as usize) * (channels as usize);
        let byte_count = sample_count * 4;
        (
            proptest::collection::vec(any::<u8>(), byte_count..=byte_count),
            Just(frames),
            Just(channels),
        )
            .prop_map(|(data, frames, channels)| AudioFrameOwned {
                data,
                frames,
                channels,
                sample_rate: 48000,
                format: AudioFormat::F32,
                timestamp_us: 0,
            })
    })
}

proptest! {
    /// 正しく構築された S16 フレームは as_s16() が Some を返す
    #[test]
    fn valid_s16_frame_returns_some(frame in arb_valid_s16_frame()) {
        let result = frame.as_s16();
        // アライメントが合わない場合は None になり得るのでスキップ
        if (frame.data.as_ptr() as usize).is_multiple_of(std::mem::align_of::<i16>()) {
            let slice = result.unwrap();
            prop_assert_eq!(slice.len(), (frame.frames as usize) * (frame.channels as usize));
        }
    }

    /// 正しく構築された F32 フレームは as_f32() が Some を返す
    #[test]
    fn valid_f32_frame_returns_some(frame in arb_valid_f32_frame()) {
        let result = frame.as_f32();
        // アライメントが合わない場合は None になり得るのでスキップ
        if (frame.data.as_ptr() as usize).is_multiple_of(std::mem::align_of::<f32>()) {
            let slice = result.unwrap();
            prop_assert_eq!(slice.len(), (frame.frames as usize) * (frame.channels as usize));
        }
    }

    /// data が短すぎる場合は None を返す
    #[test]
    fn short_data_returns_none(
        frames in 1i32..=128,
        channels in 1i32..=8,
    ) {
        let required_bytes = (frames as usize) * (channels as usize) * 2;
        // 必要量より少ないデータ
        if required_bytes > 0 {
            let short_data = vec![0u8; required_bytes - 1];
            let frame = AudioFrameOwned {
                data: short_data,
                frames,
                channels,
                sample_rate: 48000,
                format: AudioFormat::S16,
                timestamp_us: 0,
            };
            prop_assert!(frame.as_s16().is_none());
        }
    }

    /// frames <= 0 または channels <= 0 の場合は None を返す
    #[test]
    fn non_positive_metadata_returns_none(
        frames in -10i32..=0,
        channels in -10i32..=0,
    ) {
        let frame = AudioFrameOwned {
            data: vec![0u8; 1024],
            frames,
            channels,
            sample_rate: 48000,
            format: AudioFormat::S16,
            timestamp_us: 0,
        };
        prop_assert!(frame.as_s16().is_none());

        let frame_f32 = AudioFrameOwned {
            data: vec![0u8; 1024],
            frames,
            channels,
            sample_rate: 48000,
            format: AudioFormat::F32,
            timestamp_us: 0,
        };
        prop_assert!(frame_f32.as_f32().is_none());
    }
}
