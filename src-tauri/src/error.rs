//! Application error type. Serialized to the frontend as `{ kind, message, ... }`.
//! Messages must never contain secrets (tokens, client secrets).
use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppError {
    #[error("database error: {message}")]
    Database { message: String },
    #[error("secure storage error: {message}")]
    Secret { message: String },
    #[error("{platform} is not configured: {message}")]
    NotConfigured { platform: String, message: String },
    #[error("authorization error: {message}")]
    Auth { message: String, reconnect_required: bool },
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String, retryable: bool },
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("network error: {message}")]
    Network { message: String },
    #[error("unsupported: {capability}")]
    Unsupported { capability: String, reason: String, manual_alternative: Option<String> },
    #[error("invalid input: {message}")]
    Validation { message: String },
    #[error("not found: {message}")]
    NotFound { message: String },
    #[error("i/o error: {message}")]
    Io { message: String },
}

impl AppError {
    pub fn validation(m: impl Into<String>) -> Self {
        AppError::Validation { message: m.into() }
    }
    pub fn unsupported(capability: &str, reason: &str, manual: Option<&str>) -> Self {
        AppError::Unsupported { capability: capability.into(), reason: reason.into(), manual_alternative: manual.map(Into::into) }
    }
    pub fn is_retryable(&self) -> bool {
        matches!(self, AppError::RateLimited { .. } | AppError::Network { .. } | AppError::Api { retryable: true, .. })
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::NotFound { message: "record not found".into() },
            e => AppError::Database { message: e.to_string() },
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io { message: e.to_string() }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Validation { message: format!("invalid JSON: {e}") }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        // without_url(): request URLs may carry query parameters we never want in logs/UI.
        AppError::Network { message: e.without_url().to_string() }
    }
}

impl From<keyring::Error> for AppError {
    fn from(e: keyring::Error) -> Self {
        AppError::Secret { message: e.to_string() }
    }
}

pub type AppResult<T> = Result<T, AppError>;
