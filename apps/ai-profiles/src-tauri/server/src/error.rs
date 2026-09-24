//! Every failure the API reports: a status and `{"error": {code, message}}`.

use ai_profiles_core::api::{ErrorBody, ErrorDetail};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized() -> ApiError {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "This client isn't paired with the server, or was revoked.",
        )
    }

    pub fn forbidden(code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn not_found(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn invalid(message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    }

    /// The request is fine, but the state of things refuses it (409).
    pub fn conflict(code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::CONFLICT, code, message)
    }

    /// Something the host needs is missing (503): tmux, claude.
    pub fn unavailable(code: &'static str, message: impl Into<String>) -> ApiError {
        ApiError::new(StatusCode::SERVICE_UNAVAILABLE, code, message)
    }

    pub fn rate_limited() -> ApiError {
        ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many attempts. Wait a few minutes and try again.",
        )
    }

    pub fn internal(error: impl std::fmt::Display) -> ApiError {
        eprintln!("internal error: {error}");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "The server hit an error. Its log (journalctl --user -u ai-profiles-server) says more.",
        )
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> ApiError {
        ApiError::internal(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code.into(),
                message: self.message,
            },
        };
        (self.status, Json(body)).into_response()
    }
}
