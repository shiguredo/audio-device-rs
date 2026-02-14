#[derive(Debug, Clone)]
pub enum Error {
    DeviceNotFound,
    DeviceAccessDenied,
    SessionCreateFailed,
    SessionStartFailed,
    NullPointer(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DeviceNotFound => write!(f, "audio device not found"),
            Error::DeviceAccessDenied => write!(f, "audio access denied"),
            Error::SessionCreateFailed => write!(f, "failed to create audio session"),
            Error::SessionStartFailed => write!(f, "failed to start audio session"),
            Error::NullPointer(name) => write!(f, "null pointer: {}", name),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
