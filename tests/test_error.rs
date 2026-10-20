//! `src/error.rs` が提供する `Error` の公開 API に対する単体テスト。

use shiguredo_audio_device::Error;

/// `InvalidDeviceId` の表示がエラー内容を説明していること
#[test]
fn invalid_device_id_display_is_descriptive() {
    let message = Error::InvalidDeviceId.to_string();

    assert!(
        message.contains("device id"),
        "デバイス ID のエラーであることが分かること: {message}"
    );
    assert!(
        message.contains("null byte"),
        "NUL バイトが原因であることが分かること: {message}"
    );
}
