use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChaindashError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Web3 error: {0}")]
    Web3(#[from] web3::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("JSON parse error: {0}")]
    Json(String),

    #[error("URI parse error: {0}")]
    UriParse(String),

    #[error("Logger error: {0}")]
    Logger(String),

    #[error("{0}")]
    Other(String),
}

impl From<hyper::Error> for ChaindashError {
    fn from(err: hyper::Error) -> Self {
        ChaindashError::Http(err.to_string())
    }
}

impl From<hyper::http::uri::InvalidUri> for ChaindashError {
    fn from(err: hyper::http::uri::InvalidUri) -> Self {
        ChaindashError::UriParse(err.to_string())
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
