use std::{error, fmt};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Http(reqwest::Error),
    Json(serde_json::Error),
    Url(String),
    Api { status: u16, body: String },
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(err) => write!(f, "http error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::Url(msg) | Self::Invalid(msg) => f.write_str(msg),
            Self::Api { status, body } => write!(f, "api error {status}: {body}"),
        }
    }
}

impl error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
