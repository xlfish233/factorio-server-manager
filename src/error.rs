//! Unified application error type using snafu, with Axum response mapping.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum AppError {
    #[snafu(display("Database error: {source}"))]
    Db { source: sea_orm::DbErr },

    #[snafu(display("Configuration error: {msg}"))]
    Config { msg: String },

    #[snafu(display("Unauthorized: {msg}"))]
    Unauthorized { msg: String },

    #[snafu(display("Bad request: {msg}"))]
    BadRequest { msg: String },

    #[snafu(display("Forbidden: {msg}"))]
    Forbidden { msg: String },

    #[snafu(display("Conflict: {msg}"))]
    Conflict { msg: String },

    #[snafu(display("Not found: {msg}"))]
    NotFound { msg: String },
}

impl From<sea_orm::DbErr> for AppError {
    fn from(source: sea_orm::DbErr) -> Self {
        Self::Db { source }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::Config { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Db { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            AppError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            AppError::Forbidden { .. } => StatusCode::FORBIDDEN,
            AppError::Conflict { .. } => StatusCode::CONFLICT,
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
        };
        // Log the error with status; TraceLayer provides method/path via span fields.
        tracing::error!(status=%status.as_u16(), error=%self.to_string(), "request failed");
        let body = serde_json::json!({
            "error": self.to_string(),
        });
        (status, Json(body)).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
