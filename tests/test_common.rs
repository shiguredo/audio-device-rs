use shiguredo_audio_device::{Error, PlaybackFrame};

#[test]
fn from_s16_data_too_large_returns_err() {
    let len = i32::MAX as usize + 1;
    let data: &[i16] =
        unsafe { std::slice::from_raw_parts(std::ptr::NonNull::dangling().as_ptr(), len) };
    let result = PlaybackFrame::from_s16(data, 1, 48000);
    assert!(matches!(result, Err(Error::DataTooLarge(_))));
}

#[test]
fn from_f32_data_too_large_returns_err() {
    let len = i32::MAX as usize + 1;
    let data: &[f32] =
        unsafe { std::slice::from_raw_parts(std::ptr::NonNull::dangling().as_ptr(), len) };
    let result = PlaybackFrame::from_f32(data, 1, 48000);
    assert!(matches!(result, Err(Error::DataTooLarge(_))));
}

#[test]
fn try_from_int_error_display_contains_data_too_large() {
    let e: Error = i32::try_from(i64::MAX).unwrap_err().into();
    let s = e.to_string();
    assert!(s.contains("data too large"));
}
