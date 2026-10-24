use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub type AppResult<T> = Result<T, AppError>;

/// Error type for the dashboard JSON API. Renders as `{"error": code, "message": text}`.
///
/// OAuth endpoints use their own RFC 6749 error format and MCP tools report
/// failures as tool results, so neither goes through this type.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("authentication required")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    /// HackMD rejected the API token.
    #[error(
        "HackMD rejected this API token. Check that it was copied in full and has not been deleted."
    )]
    InvalidToken,
    /// HackMD failed or refused the request.
    #[error("{0}")]
    Hackmd(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Internal(other.into()),
        }
    }
}

impl From<hackmd::Error> for AppError {
    fn from(err: hackmd::Error) -> Self {
        match err {
            hackmd::Error::InvalidToken => Self::InvalidToken,
            other => Self::Hackmd(other.to_string()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::InvalidToken => (StatusCode::BAD_REQUEST, "invalid_token"),
            Self::Hackmd(_) => (StatusCode::BAD_GATEWAY, "hackmd_error"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        let message = match &self {
            Self::Internal(err) => {
                tracing::error!(error = ?err, "internal error");
                "internal server error".to_owned()
            }
            other => other.to_string(),
        };
        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}
