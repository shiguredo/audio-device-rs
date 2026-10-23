//! `src/error.rs` が定義する `Error` の Property-Based Testing。
//!
//! 全バリアントの `Display` 出力と `std::error::Error` 実装を検証する。

use std::cell::Cell;

use shiguredo_audio_device::Error;

/// 各 PBT 共通のシード取得用環境変数名
///
/// 失敗時に表示される hex シードをそのまま代入して再現する。
const SEED_ENV: &str = "AUDIO_DEVICE_PBT_SEED";

/// デフォルトのケースバジェット
const CASES: usize = 256;

/// `Error` のバリアントを生成する関数の型
type ErrorSampler = fn(&mut noprop::TestCaseContext) -> Error;

/// `Error` の全バリアントを生成する関数の一覧
const ERROR_SAMPLERS: &[ErrorSampler] = &[
    |_| Error::DeviceNotFound,
    |_| Error::DeviceAccessDenied,
    |_| Error::SessionCreateFailed,
    |_| Error::SessionStartFailed,
    |_| Error::InvalidChannels,
    |_| Error::InvalidDataLength,
    |_| Error::DataTooLarge(i8::try_from(256i16).expect_err("256 は i8 に収まらない")),
    |ctx| Error::UnknownFormat(noprop::sample_i32(ctx)),
    |ctx| Error::UnknownDeviceType(noprop::sample_i32(ctx)),
    |_| Error::NullPointer("device name"),
    |_| Error::InvalidDeviceId,
    |_| Error::ComInitFailed,
];

/// `Error` の全バリアントを生成する
///
/// 到達ゲートで全バリアントが実際に生成されたことを確認できるよう、
/// `sample_error` の戻り値と併せてバリアント番号も返す。
fn sample_error(ctx: &mut noprop::TestCaseContext) -> (Error, usize) {
    let index = noprop::sample_usize_in(ctx, 0..ERROR_SAMPLERS.len());
    (ERROR_SAMPLERS[index](ctx), index)
}

/// 全バリアントの Display 出力が空でない
#[test]
fn prop_error_display_is_non_empty() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let reached = (0..ERROR_SAMPLERS.len())
        .map(|_| Cell::new(0usize))
        .collect::<Vec<_>>();
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let (err, index) = sample_error(ctx);
        let msg = err.to_string();
        assert!(!msg.is_empty(), "Display 出力が空でないこと: {err:?}");
        // 到達ゲートは Display の検証が成功した後に数える。
        reached[index].set(reached[index].get() + 1);
        Ok(())
    })?;
    let unreached: Vec<usize> = (0..ERROR_SAMPLERS.len())
        .filter(|&i| reached[i].get() == 0)
        .collect();
    assert!(
        unreached.is_empty(),
        "全バリアントの Display が検証されること (未到達: {unreached:?})\n{runner}"
    );
    Ok(())
}

/// `NullPointer` の Display 出力に名前が反映される
#[test]
fn prop_null_pointer_contains_value() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..=16);
        let name = noprop::sample_ascii_printable_string(ctx, len);
        let leaked: &'static str = Box::leak(name.clone().into_boxed_str());
        let msg = Error::NullPointer(leaked).to_string();
        assert!(
            msg.starts_with("null pointer: "),
            "接頭辞が一致すること: {msg}"
        );
        assert!(msg.contains(&name), "名前が含まれること: {msg}");
        Ok(())
    })?;
    Ok(())
}

/// 未知のフォーマット値・デバイス種別値が Display 出力に反映される
#[test]
fn prop_unknown_values_contain_number() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let format = noprop::sample_i32(ctx);
        let msg = Error::UnknownFormat(format).to_string();
        assert!(
            msg.contains(&format.to_string()),
            "フォーマット値が含まれること: {msg}"
        );

        let device_type = noprop::sample_i32(ctx);
        let msg = Error::UnknownDeviceType(device_type).to_string();
        assert!(
            msg.contains(&device_type.to_string()),
            "デバイス種別値が含まれること: {msg}"
        );
        Ok(())
    })?;
    Ok(())
}

/// `DataTooLarge` の Display 出力に内部エラーの説明が反映される
#[test]
fn prop_data_too_large_contains_inner() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let checked = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        // i32 に収まらない値だけを対象にする。収まる値は対象外。
        let value = noprop::sample_i64(ctx);
        let inner = match i32::try_from(value) {
            Ok(_) => return Ok(()),
            Err(e) => e,
        };
        let msg = Error::DataTooLarge(inner).to_string();
        assert!(
            msg.starts_with("data too large: "),
            "接頭辞が一致すること: {msg}"
        );
        // 到達ゲートは不変条件の評価が成功した後に数える。
        checked.set(checked.get() + 1);
        Ok(())
    })?;
    assert!(
        checked.get() > 0,
        "i32 に収まらない値のケースが 1 件以上検証されること\n{runner}"
    );
    Ok(())
}

/// `Error::source()` は常に None
#[test]
fn prop_error_source_is_none() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        use std::error::Error as StdError;
        let (err, _) = sample_error(ctx);
        assert!(err.source().is_none(), "source() は常に None を返すこと");
        Ok(())
    })?;
    Ok(())
}
