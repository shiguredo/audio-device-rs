use proptest::prelude::*;
use shiguredo_audio_device::Error;

fn arb_error() -> impl Strategy<Value = Error> {
    let data_too_large = Just(Error::DataTooLarge(
        i8::try_from(256i16).expect_err("256 should not fit in i8"),
    ));
    let null_pointer = ".*".prop_map(|s| Error::NullPointer(Box::leak(s.into_boxed_str())));

    prop_oneof![
        Just(Error::DeviceNotFound),
        Just(Error::DeviceAccessDenied),
        Just(Error::SessionCreateFailed),
        Just(Error::SessionStartFailed),
        Just(Error::InvalidChannels),
        data_too_large,
        any::<i32>().prop_map(Error::UnknownFormat),
        any::<i32>().prop_map(Error::UnknownDeviceType),
        null_pointer,
        Just(Error::ComInitFailed),
    ]
}

proptest! {
    /// Display 出力が空でない
    #[test]
    fn display_is_non_empty(err in arb_error()) {
        let msg = err.to_string();
        prop_assert!(!msg.is_empty());
    }

    /// NullPointer の Display 出力にパラメータが反映される
    #[test]
    fn null_pointer_contains_value(name in ".*") {
        let err = Error::NullPointer(Box::leak(name.clone().into_boxed_str()));
        let msg = err.to_string();
        prop_assert!(msg.starts_with("null pointer: "));
        prop_assert!(msg.contains(&name));
    }

    /// Error::source() は常に None
    #[test]
    fn source_is_none(err in arb_error()) {
        use std::error::Error as StdError;
        prop_assert!(err.source().is_none());
    }
}
