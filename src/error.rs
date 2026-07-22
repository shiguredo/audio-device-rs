#[derive(Debug, Clone)]
pub enum Error {
    DeviceNotFound,
    DeviceAccessDenied,
    SessionCreateFailed,
    SessionStartFailed,
    ComInitFailed,
    NullPointer(&'static str),
    InvalidChannels,
    DataTooLarge(std::num::TryFromIntError),
    UnknownFormat(i32),
    UnknownDeviceType(i32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

pub type Result<T> = std::result::Result<T, Error>;
