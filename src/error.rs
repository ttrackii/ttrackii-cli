use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("not authenticated; run `ttrackii login`")]
    AuthRequired,
    #[error("authentication failed: {0}")]
    Authentication(String),
    #[error("API request failed ({status}): {message}")]
    Api { status: StatusCode, message: String },
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("secure credential storage is unavailable: {0}")]
    SecureStorage(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unexpected server response: {0}")]
    Response(String),
}

pub type Result<T> = std::result::Result<T, CliError>;
