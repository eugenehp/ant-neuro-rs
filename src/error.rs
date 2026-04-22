use thiserror::Error;

#[derive(Debug, Error)]
pub enum AntNeuroError {
    #[error("device not connected")]
    NotConnected,
    #[error("resource already exists")]
    AlreadyExists,
    #[error("not found")]
    NotFound,
    #[error("incorrect value")]
    IncorrectValue,
    #[error("internal SDK error")]
    InternalError,
    #[error("unknown SDK error (rc={0})")]
    Unknown(i32),
    #[cfg(feature = "native")]
    #[error("USB error: {0}")]
    Usb(#[from] rusb::Error),
    #[cfg(feature = "ffi")]
    #[error("library loading error: {0}")]
    LibLoading(#[from] libloading::Error),
    #[error("SDK version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: i32, actual: i32 },
    #[error("no amplifiers found")]
    NoAmplifiers,
    #[error("stream error: no data available")]
    NoData,
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("channel send error")]
    ChannelClosed,
}

pub type Result<T> = std::result::Result<T, AntNeuroError>;

/// Convert a raw SDK return code into a Result.
pub(crate) fn check_rc(rc: i32) -> Result<i32> {
    match rc {
        rc if rc >= 0 => Ok(rc),
        -1 => Err(AntNeuroError::NotConnected),
        -2 => Err(AntNeuroError::AlreadyExists),
        -3 => Err(AntNeuroError::NotFound),
        -4 => Err(AntNeuroError::IncorrectValue),
        -5 => Err(AntNeuroError::InternalError),
        other => Err(AntNeuroError::Unknown(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_rc_positive() {
        assert_eq!(check_rc(0).unwrap(), 0);
        assert_eq!(check_rc(1).unwrap(), 1);
        assert_eq!(check_rc(42).unwrap(), 42);
    }

    #[test]
    fn test_check_rc_not_connected() {
        let err = check_rc(-1).unwrap_err();
        assert!(matches!(err, AntNeuroError::NotConnected));
    }

    #[test]
    fn test_check_rc_already_exists() {
        let err = check_rc(-2).unwrap_err();
        assert!(matches!(err, AntNeuroError::AlreadyExists));
    }

    #[test]
    fn test_check_rc_not_found() {
        let err = check_rc(-3).unwrap_err();
        assert!(matches!(err, AntNeuroError::NotFound));
    }

    #[test]
    fn test_check_rc_incorrect_value() {
        let err = check_rc(-4).unwrap_err();
        assert!(matches!(err, AntNeuroError::IncorrectValue));
    }

    #[test]
    fn test_check_rc_internal_error() {
        let err = check_rc(-5).unwrap_err();
        assert!(matches!(err, AntNeuroError::InternalError));
    }

    #[test]
    fn test_check_rc_unknown() {
        let err = check_rc(-99).unwrap_err();
        assert!(matches!(err, AntNeuroError::Unknown(-99)));
    }

    #[test]
    fn test_display_strings() {
        assert_eq!(format!("{}", AntNeuroError::NotConnected), "device not connected");
        assert_eq!(format!("{}", AntNeuroError::AlreadyExists), "resource already exists");
        assert_eq!(format!("{}", AntNeuroError::NotFound), "not found");
        assert_eq!(format!("{}", AntNeuroError::IncorrectValue), "incorrect value");
        assert_eq!(format!("{}", AntNeuroError::InternalError), "internal SDK error");
        assert_eq!(format!("{}", AntNeuroError::Unknown(-42)), "unknown SDK error (rc=-42)");
    }
}
