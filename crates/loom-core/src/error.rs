use serde::Serialize;

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    NotFound,
    Conflict,
    Busy,
    CorruptStore,
    StorageUnavailable,
    StaleLease,
}

#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreError {
    pub code: ErrorCode,
    pub message: String,
}
impl CoreError {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }
    pub(crate) fn corrupt(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::CorruptStore, message)
    }
    pub(crate) fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }
    pub(crate) fn missing(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }
}
impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for CoreError {}
impl From<std::io::Error> for CoreError {
    fn from(error: std::io::Error) -> Self {
        Self::new(ErrorCode::StorageUnavailable, error.to_string())
    }
}
pub type Result<T> = std::result::Result<T, CoreError>;
