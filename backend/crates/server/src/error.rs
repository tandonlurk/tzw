//! API errors: `{"error":{"code","message","details":[{path,message,code}]}}`.

use aen_core::ledger::LedgerError;
use axum::Json;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

/// One problem with one field. `path` is a dotted field path such as
/// `policy.max_per_interaction` or `graph.nodes.n2`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Detail {
    pub path: String,
    pub message: String,
    pub code: String,
}

impl Detail {
    pub fn new(
        path: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Detail {
            path: path.into(),
            message: message.into(),
            code: code.into(),
        }
    }
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Vec<Detail>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        ApiError {
            status,
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }

    pub fn not_found(what: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("{what} not found"),
        )
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    /// 422 with one entry per invalid field.
    pub fn validation(details: Vec<Detail>) -> Self {
        ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "validation_failed",
            message: "the request is not valid".into(),
            details,
        }
    }

    pub fn unprocessable(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    /// Logs the cause and returns a generic 500: internals are not leaked.
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        tracing::error!(%cause, "internal error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "internal server error",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": { "code": self.code, "message": self.message, "details": self.details }
        });
        (self.status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::internal(e)
    }
}

impl From<PathRejection> for ApiError {
    fn from(e: PathRejection) -> Self {
        ApiError::bad_request(e.body_text())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(e: QueryRejection) -> Self {
        ApiError::bad_request(e.body_text())
    }
}

impl From<LedgerError> for ApiError {
    fn from(e: LedgerError) -> Self {
        match e {
            LedgerError::InsufficientFunds { .. } => {
                ApiError::unprocessable("insufficient_funds", e.to_string())
            }
            LedgerError::ZeroAmount | LedgerError::Overflow => {
                ApiError::validation(vec![Detail::new("amount", "invalid_amount", e.to_string())])
            }
            // Anything else means the server built a bad posting set.
            other => ApiError::internal(other),
        }
    }
}
