//! `src/common.rs` が定義する共通型の Property-Based Testing。
//!
//! `AudioFrameOwned` / `AudioFrame` / `PlaybackFrame` のフォーマット変換と
//! ラウンドトリップを検証する。

use std::cell::Cell;

use shiguredo_audio_device::{AudioFormat, AudioFrameOwned, Error, PlaybackFrame};

/// 各 PBT 共通のシード取得用環境変数名
///
/// 失敗時に表示される hex シードをそのまま代入して再現する。
const SEED_ENV: &str = "AUDIO_DEVICE_PBT_SEED";

/// デフォルトのケースバジェット
const CASES: usize = 256;

// ---------------------------------------------------------------------------
// 入力生成
// ---------------------------------------------------------------------------

/// 音声フォーマットを生成する
fn sample_format(ctx: &mut noprop::TestCaseContext) -> AudioFormat {
    match noprop::sample_weighted_index(ctx, &[1, 1]) {
        0 => AudioFormat::S16,
        _ => AudioFormat::F32,
    }
}

/// 1 サンプルあたりのバイト数を返す
fn bytes_per_sample(format: AudioFormat) -> usize {
    match format {
        AudioFormat::S16 => 2,
        AudioFormat::F32 => 4,
    }
}

/// 妥当な `AudioFrameOwned` を生成する
///
/// フレーム数とチャンネル数を先に決めてから、その積とサンプルサイズにちょうど
/// 一致するバイト列を生成する。`frames` と `channels` は 1 以上であり、
/// `as_s16()` / `as_f32()` が参照できる最小構成になっている。
fn sample_valid_frame(ctx: &mut noprop::TestCaseContext) -> AudioFrameOwned {
    let frames = noprop::sample_usize_in(ctx, 1..=128) as i32;
    let channels = noprop::sample_usize_in(ctx, 1..=8) as i32;
    let format = sample_format(ctx);
    let byte_count = (frames as usize) * (channels as usize) * bytes_per_sample(format);
    AudioFrameOwned {
        data: noprop::sample_bytes_vec(ctx, byte_count),
        frames,
        channels,
        sample_rate: 48000,
        format,
        timestamp_us: 0,
    }
}

/// フレームのフォーマットに対応する参照スライスの長さを返す
///
/// S16 と F32 で戻り値の型が異なるため、長さだけを取り出して比較できる形にする。
fn frame_len(frame: &AudioFrameOwned) -> Option<usize> {
    match frame.format {
        AudioFormat::S16 => frame.as_s16().map(<[i16]>::len),
        AudioFormat::F32 => frame.as_f32().map(<[f32]>::len),
    }
}

// ---------------------------------------------------------------------------
// AudioFrameOwned
// ---------------------------------------------------------------------------

/// `AudioFrameOwned` のラウンドトリップ
///
/// `as_frame()` で参照フレームに変換し、`to_owned()` で所有フレームに戻すと
/// 元の値と等しくなることを確認する。S16 と F32 の両方のフォーマットが
/// 実際に生成されたことを到達ゲートで確認する。
#[test]
fn prop_audio_frame_owned_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let s16_count = Cell::new(0usize);
    let f32_count = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let frame = sample_valid_frame(ctx);
        let roundtripped = frame.as_frame().to_owned();
        assert_eq!(
            roundtripped, frame,
            "as_frame() → to_owned() のラウンドトリップで元のフレームに戻ること"
        );
        // 到達ゲートは不変条件の評価が成功した後に数える。
        match frame.format {
            AudioFormat::S16 => s16_count.set(s16_count.get() + 1),
            AudioFormat::F32 => f32_count.set(f32_count.get() + 1),
        }
        Ok(())
    })?;
    assert!(
        s16_count.get() > 0 && f32_count.get() > 0,
        "S16 と F32 の両方のフォーマットでラウンドトリップを検証すること \
         (S16: {}, F32: {})\n{runner}",
        s16_count.get(),
        f32_count.get()
    );
    Ok(())
}

/// 妥当なフレームはフォーマットに対応するアクセサが Some を返す
///
/// `data` のアライメントが合わない環境では None になり得るため、アライメントが
/// 合う場合のみサンプル数を検証する。アライメントが合ったケースが実際に
/// あったことを到達ゲートで確認する。
#[test]
fn prop_valid_frame_returns_some() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let checked = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let frame = sample_valid_frame(ctx);
        let expected_len = (frame.frames as usize) * (frame.channels as usize);
        let aligned = match frame.format {
            AudioFormat::S16 => {
                (frame.data.as_ptr() as usize).is_multiple_of(std::mem::align_of::<i16>())
            }
            AudioFormat::F32 => {
                (frame.data.as_ptr() as usize).is_multiple_of(std::mem::align_of::<f32>())
            }
        };
        if aligned {
            let len = frame_len(&frame).expect("アライメントが合っていれば Some を返す");
            assert_eq!(
                len, expected_len,
                "サンプル数が frames * channels と一致すること"
            );
            // 到達ゲートは不変条件の評価が成功した後に数える。
            checked.set(checked.get() + 1);
        }
        Ok(())
    })?;
    assert!(
        checked.get() > 0,
        "アライメントが合うケースが 1 件以上検証されること\n{runner}"
    );
    Ok(())
}

/// data が必要バイト数に満たない場合は None を返す
#[test]
fn prop_short_data_returns_none() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let frames = noprop::sample_usize_in(ctx, 1..=128) as i32;
        let channels = noprop::sample_usize_in(ctx, 1..=8) as i32;
        let format = sample_format(ctx);
        let required_bytes = (frames as usize) * (channels as usize) * bytes_per_sample(format);
        // 必要量より 1 バイト少ないデータを生成する。
        let frame = AudioFrameOwned {
            data: noprop::sample_bytes_vec(ctx, required_bytes - 1),
            frames,
            channels,
            sample_rate: 48000,
            format,
            timestamp_us: 0,
        };
        assert!(
            frame_len(&frame).is_none(),
            "data が必要量に満たない場合は None を返すこと"
        );
        Ok(())
    })?;
    Ok(())
}

/// frames <= 0 または channels <= 0 の場合は None を返す
#[test]
fn prop_non_positive_metadata_returns_none() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let frames = noprop::sample_i32_in(ctx, -10..=0);
        let channels = noprop::sample_i32_in(ctx, -10..=0);
        for format in [AudioFormat::S16, AudioFormat::F32] {
            let frame = AudioFrameOwned {
                data: vec![0u8; 1024],
                frames,
                channels,
                sample_rate: 48000,
                format,
                timestamp_us: 0,
            };
            assert!(
                frame_len(&frame).is_none(),
                "frames または channels が 0 以下なら None を返すこと: {format:?}"
            );
        }
        Ok(())
    })?;
    Ok(())
}

/// 構築したフォーマット以外のアクセサは None を返す
#[test]
fn prop_mismatched_format_returns_none() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let frame = sample_valid_frame(ctx);
        match frame.format {
            AudioFormat::S16 => assert!(
                frame.as_f32().is_none(),
                "S16 フレームを F32 として取得できないこと"
            ),
            AudioFormat::F32 => assert!(
                frame.as_s16().is_none(),
                "F32 フレームを S16 として取得できないこと"
            ),
        }
        Ok(())
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// PlaybackFrame
// ---------------------------------------------------------------------------

/// 妥当な入力で `PlaybackFrame::from_s16` が Ok を返す
///
/// `frames` は `data.len() / channels` の切り捨て除算で決まるため、
/// 割り切れない場合も含めて検証する。
#[test]
fn prop_from_s16_valid_channels_returns_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let channels = noprop::sample_i32_in(ctx, 1..=32);
        let sample_rate = noprop::sample_choice(ctx, &[8000, 16000, 44100, 48000]);
        let sample_count = noprop::sample_usize_in(ctx, 0..=512);
        let data: Vec<i16> = (0..sample_count).map(|_| noprop::sample_i16(ctx)).collect();

        let frame = PlaybackFrame::from_s16(&data, channels, sample_rate)
            .expect("channels > 0 なら Ok を返す");
        let expected_frames =
            i32::try_from(data.len()).expect("テストのデータ長は i32 に収まる") / channels;
        assert_eq!(frame.frames, expected_frames, "frames が一致すること");
        assert_eq!(frame.channels, channels, "channels が一致すること");
        assert_eq!(frame.sample_rate, sample_rate, "sample_rate が一致すること");
        assert_eq!(frame.format, AudioFormat::S16, "format が S16 であること");
        assert_eq!(
            frame.data.len(),
            data.len() * 2,
            "入力サンプル数から期待されるバイト数と一致すること"
        );
        Ok(())
    })?;
    Ok(())
}

/// 妥当な入力で `PlaybackFrame::from_f32` が Ok を返す
#[test]
fn prop_from_f32_valid_channels_returns_ok() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let channels = noprop::sample_i32_in(ctx, 1..=32);
        let sample_rate = noprop::sample_choice(ctx, &[8000, 16000, 44100, 48000]);
        let sample_count = noprop::sample_usize_in(ctx, 0..=512);
        let data: Vec<f32> = (0..sample_count).map(|_| noprop::sample_f32(ctx)).collect();

        let frame = PlaybackFrame::from_f32(&data, channels, sample_rate)
            .expect("channels > 0 なら Ok を返す");
        let expected_frames =
            i32::try_from(data.len()).expect("テストのデータ長は i32 に収まる") / channels;
        assert_eq!(frame.frames, expected_frames, "frames が一致すること");
        assert_eq!(frame.channels, channels, "channels が一致すること");
        assert_eq!(frame.sample_rate, sample_rate, "sample_rate が一致すること");
        assert_eq!(frame.format, AudioFormat::F32, "format が F32 であること");
        assert_eq!(
            frame.data.len(),
            data.len() * 4,
            "入力サンプル数から期待されるバイト数と一致すること"
        );
        Ok(())
    })?;
    Ok(())
}

/// channels <= 0 なら `from_s16` と `from_f32` は `InvalidChannels` を返す
#[test]
fn prop_invalid_channels_returns_err() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let channels = noprop::sample_i32_in(ctx, -10..=0);
        let data_s16: Vec<i16> = (0..16).map(|_| noprop::sample_i16(ctx)).collect();
        let result_s16 = PlaybackFrame::from_s16(&data_s16, channels, 48000);
        assert!(
            matches!(result_s16, Err(Error::InvalidChannels)),
            "channels <= 0 なら from_s16 は InvalidChannels を返すこと"
        );

        let data_f32: Vec<f32> = (0..16).map(|_| noprop::sample_f32(ctx)).collect();
        let result_f32 = PlaybackFrame::from_f32(&data_f32, channels, 48000);
        assert!(
            matches!(result_f32, Err(Error::InvalidChannels)),
            "channels <= 0 なら from_f32 は InvalidChannels を返すこと"
        );
        Ok(())
    })?;
    Ok(())
}
