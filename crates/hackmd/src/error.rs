//! Errors whose `Display` is safe to show the agent: they never contain the
//! token and summarize HackMD's error bodies instead of echoing them.

use reqwest::StatusCode;
use serde_json::Value;

use crate::patch::PatchError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HackMD rejected the API token: it was revoked or deleted.
    #[error("HackMD rejected the API token")]
    InvalidToken,
    #[error("HackMD denied access: {0}")]
    Forbidden(String),
    #[error("not found on HackMD: {0}")]
    NotFound(String),
    #[error("HackMD rejected the request: {0}")]
    Rejected(String),
    #[error("conflict on HackMD: {0}")]
    Conflict(String),
    #[error("HackMD rate limit reached{}", retry_hint(*.retry_after))]
    RateLimited { retry_after: Option<u64> },
    #[error("HackMD returned status {status}: {message}")]
    Api { status: u16, message: String },
    #[error("could not reach HackMD: {0}")]
    Network(String),
    #[error("unexpected response from HackMD: {0}")]
    BadResponse(String),
    #[error("{0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Patch(#[from] PatchError),
}

fn retry_hint(retry_after: Option<u64>) -> String {
    retry_after
        .map(|seconds| format!("; retry in {seconds} seconds"))
        .unwrap_or_default()
}

/// Longest error message kept from a HackMD response body.
const MAX_MESSAGE: usize = 300;

impl Error {
    pub(crate) fn network(err: reqwest::Error) -> Self {
        // `without_url` keeps request URLs (and their IDs) out of logs.
        Self::Network(err.without_url().to_string())
    }

    pub(crate) fn from_status(status: StatusCode, body: &[u8], retry_after: Option<u64>) -> Self {
        let message = summarize(body)
            .unwrap_or_else(|| status.canonical_reason().unwrap_or("error").to_owned());
        match status {
            StatusCode::UNAUTHORIZED => Self::InvalidToken,
            StatusCode::FORBIDDEN => Self::Forbidden(message),
            StatusCode::NOT_FOUND => Self::NotFound(message),
            StatusCode::CONFLICT => Self::Conflict(message),
            StatusCode::TOO_MANY_REQUESTS => Self::RateLimited { retry_after },
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => Self::Rejected(message),
            _ => Self::Api {
                status: status.as_u16(),
                message,
            },
        }
    }
}

/// The human-readable part of an error body, bounded in length.
fn summarize(body: &[u8]) -> Option<String> {
    let text = match serde_json::from_slice::<Value>(body) {
        Ok(json) => ["message", "error", "detail", "code"]
            .iter()
            .find_map(|key| match json.get(key)? {
                Value::String(text) => Some(text.clone()),
                Value::Object(inner) => inner.get("message")?.as_str().map(str::to_owned),
                _ => None,
            })?,
        Err(_) => String::from_utf8_lossy(body).trim().to_owned(),
    };
    // HTML error pages from proxies say nothing useful.
    if text.is_empty() || text.starts_with('<') {
        return None;
    }
    Some(match text.char_indices().nth(MAX_MESSAGE) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_variants() {
        let err = Error::from_status(StatusCode::UNAUTHORIZED, b"{}", None);
        assert!(matches!(err, Error::InvalidToken));
        let err = Error::from_status(
            StatusCode::CONFLICT,
            br#"{"error":"no_content_change","message":"Nothing changed"}"#,
            None,
        );
        assert_eq!(err.to_string(), "conflict on HackMD: Nothing changed");
        let err = Error::from_status(StatusCode::TOO_MANY_REQUESTS, b"", Some(30));
        assert_eq!(
            err.to_string(),
            "HackMD rate limit reached; retry in 30 seconds"
        );
    }

    #[test]
    fn bodies_are_summarized() {
        let err = Error::from_status(StatusCode::BAD_GATEWAY, b"<html>oops</html>", None);
        assert_eq!(err.to_string(), "HackMD returned status 502: Bad Gateway");
        let long = "x".repeat(1000);
        let err = Error::from_status(StatusCode::BAD_REQUEST, long.as_bytes(), None);
        assert!(err.to_string().chars().count() < 400);
    }
}
