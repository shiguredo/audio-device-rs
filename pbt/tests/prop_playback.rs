use proptest::prelude::*;
use shiguredo_audio_device::PlaybackFrame;

proptest! {
    /// channels > 0 なら from_s16 は Ok を返す
    #[test]
    fn from_s16_valid_channels_returns_ok(
        channels in 1i32..=32,
        sample_rate in prop_oneof![Just(8000), Just(16000), Just(44100), Just(48000)],
        data in proptest::collection::vec(any::<i16>(), 0..=512),
    ) {
        let frame = PlaybackFrame::from_s16(&data, channels, sample_rate).unwrap();
        let expected_frames = data.len() as i32 / channels;
        prop_assert_eq!(frame.frames, expected_frames);
        prop_assert_eq!(frame.channels, channels);
        prop_assert_eq!(frame.data.len(), data.len() * 2);
    }

    /// channels > 0 なら from_f32 は Ok を返す
    #[test]
    fn from_f32_valid_channels_returns_ok(
        channels in 1i32..=32,
        sample_rate in prop_oneof![Just(8000), Just(16000), Just(44100), Just(48000)],
        data in proptest::collection::vec(any::<f32>(), 0..=512),
    ) {
        let frame = PlaybackFrame::from_f32(&data, channels, sample_rate).unwrap();
        let expected_frames = data.len() as i32 / channels;
        prop_assert_eq!(frame.frames, expected_frames);
        prop_assert_eq!(frame.channels, channels);
        prop_assert_eq!(frame.data.len(), data.len() * 4);
    }

    /// channels <= 0 なら from_s16 は Err を返す
    #[test]
    fn from_s16_invalid_channels_returns_err(
        channels in -10i32..=0,
        data in proptest::collection::vec(any::<i16>(), 0..=64),
    ) {
        prop_assert!(PlaybackFrame::from_s16(&data, channels, 48000).is_err());
    }

    /// channels <= 0 なら from_f32 は Err を返す
    #[test]
    fn from_f32_invalid_channels_returns_err(
        channels in -10i32..=0,
        data in proptest::collection::vec(any::<f32>(), 0..=64),
    ) {
        prop_assert!(PlaybackFrame::from_f32(&data, channels, 48000).is_err());
    }
}
