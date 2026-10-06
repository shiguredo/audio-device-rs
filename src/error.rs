//! このクレートで使用するエラー型の定義。

use std::fmt;

/// このクレートのエラー型。
#[derive(Debug, Clone, Copy)]
pub enum Error {
    /// 指定されたオーディオデバイスが見つからない
    DeviceNotFound,
    /// オーディオデバイスへのアクセスが拒否された
    DeviceAccessDenied,
    /// オーディオセッションの作成に失敗した
    SessionCreateFailed,
    /// オーディオセッションの開始に失敗した
    SessionStartFailed,
    /// COM の初期化に失敗した
    ComInitFailed,
    /// C 側から null ポインタが返された
    ///
    /// フィールドは null だった対象の名前。
    NullPointer(&'static str),
    /// チャンネル数が 0 以下
    InvalidChannels,
    /// データ長が i32 に収まらない
    DataTooLarge(std::num::TryFromIntError),
    /// 未知のオーディオフォーマット値
    UnknownFormat(i32),
    /// 未知のオーディオデバイス種別値
    UnknownDeviceType(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::DeviceNotFound => write!(f, "audio device not found"),
            Error::DeviceAccessDenied => write!(f, "audio access denied"),
            Error::SessionCreateFailed => write!(f, "failed to create audio session"),
            Error::SessionStartFailed => write!(f, "failed to start audio session"),
            Error::ComInitFailed => write!(
                f,
                "COM initialization failed: thread has incompatible apartment model"
            ),
            Error::NullPointer(name) => write!(f, "null pointer: {}", name),
            Error::InvalidChannels => write!(f, "invalid channels: must be greater than 0"),
            Error::DataTooLarge(e) => write!(f, "data too large: {}", e),
            Error::UnknownFormat(v) => write!(f, "unknown audio format: {}", v),
            Error::UnknownDeviceType(v) => write!(f, "unknown audio device type: {}", v),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::num::TryFromIntError> for Error {
    fn from(e: std::num::TryFromIntError) -> Self {
        Error::DataTooLarge(e)
    }
}

/// このクレートの処理結果。
pub type Result<T> = std::result::Result<T, Error>;
