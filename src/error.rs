use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChaindashError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("JSON parse error: {0}")]
    Json(String),

    #[error("Logger error: {0}")]
    Logger(String),

    #[error("Ctrl+C handler error: {0}")]
    Ctrlc(String),

    #[error("Terminal error: {0}")]
    Terminal(String),

    #[error("{0}")]
    Other(String),
}

impl From<alloy::transports::TransportError> for ChaindashError {
    fn from(err: alloy::transports::TransportError) -> Self {
        ChaindashError::Rpc(err.to_string())
    }
}

impl From<reqwest::Error> for ChaindashError {
    fn from(err: reqwest::Error) -> Self {
        ChaindashError::Http(err.to_string())
    }
}

impl From<serde_json::Error> for ChaindashError {
    fn from(err: serde_json::Error) -> Self {
        ChaindashError::Json(err.to_string())
    }
}

impl From<String> for ChaindashError {
    fn from(err: String) -> Self {
        ChaindashError::Other(err)
    }
}

impl From<&str> for ChaindashError {
    fn from(err: &str) -> Self {
        ChaindashError::Other(err.to_string())
    }
}

impl From<log::SetLoggerError> for ChaindashError {
    fn from(err: log::SetLoggerError) -> Self {
        ChaindashError::Logger(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ChaindashError>;
