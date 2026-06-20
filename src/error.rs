#[derive(Debug, Clone)]
pub enum Error {
    DeviceNotFound,
    DeviceAccessDenied,
    SessionCreateFailed,
    SessionStartFailed,
    #[cfg(target_os = "windows")]
    ComInitFailed,
    NullPointer(&'static str),
    InvalidChannels,
    DataTooLarge,
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
            #[cfg(target_os = "windows")]
            Error::ComInitFailed => write!(
                f,
                "COM initialization failed: thread has incompatible apartment model"
            ),
            Error::NullPointer(name) => write!(f, "null pointer: {}", name),
            Error::InvalidChannels => write!(f, "invalid channels: must be greater than 0"),
            Error::UnknownFormat(v) => write!(f, "unknown audio format: {}", v),
            Error::UnknownDeviceType(v) => write!(f, "unknown audio device type: {}", v),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
