use serde::{Deserialize, Serialize};
use std::{fmt, io};

pub type Result<T> = std::result::Result<T, SkillsError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    Conflict,
    LockBusy,
    Updating,
    RecoveryRequired,
    PermissionDenied,
    Io,
    Cancelled,
    Timeout,
    SourceUnavailable,
    AuthRequired,
    RateLimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsError {
    pub code: ErrorCode,
    pub message: String,
}

impl SkillsError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub fn recovery(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::RecoveryRequired, message)
    }
}

impl fmt::Display for SkillsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SkillsError {}

impl From<io::Error> for SkillsError {
    fn from(error: io::Error) -> Self {
        let code = if error.kind() == io::ErrorKind::PermissionDenied {
            ErrorCode::PermissionDenied
        } else {
            ErrorCode::Io
        };
        Self::new(code, error.to_string())
    }
}
