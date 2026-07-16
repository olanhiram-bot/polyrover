//! Error taxonomy with retry and rate-limit classification.

use std::{error, fmt};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Http(String),
    Json(serde_json::Error),
    Url(String),
    Api { status: u16, body: String },
    RateLimited { retry_after_secs: Option<u64> },
    WebSocket(String),
    ReconnectExhausted { attempts: u32, last_error: String },
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(err) => write!(f, "http error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::Url(msg) | Self::Invalid(msg) => f.write_str(msg),
            Self::Api { status, body } => write!(f, "api error {status}: {body}"),
            Self::RateLimited { retry_after_secs } => match retry_after_secs {
                Some(seconds) => write!(f, "api rate limited; retry after {seconds}s"),
                None => f.write_str("api rate limited"),
            },
            Self::WebSocket(msg) => write!(f, "websocket error: {msg}"),
            Self::ReconnectExhausted {
                attempts,
                last_error,
            } => write!(
                f,
                "websocket reconnect exhausted after {attempts} attempts: {last_error}"
            ),
        }
    }
}

impl error::Error for Error {}

#[cfg(feature = "public")]
impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_exhaustion_is_typed_and_redacted() {
        let err = Error::ReconnectExhausted {
            attempts: 3,
            last_error: "connection reset".into(),
        };
        assert_eq!(
            err.to_string(),
            "websocket reconnect exhausted after 3 attempts: connection reset"
        );
    }
}
