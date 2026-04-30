use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::types::BadRequestResponse;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{message}")]
    BadRequest { message: String, code: String },
    #[error("{message}")]
    NotFound { message: String, code: String },
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
            code: code.into(),
        }
    }

    pub fn not_found(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
            code: code.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message, code) = match self {
            Self::BadRequest { message, code } => (StatusCode::BAD_REQUEST, message, code),
            Self::NotFound { message, code } => (StatusCode::NOT_FOUND, message, code),
        };

        (status, Json(BadRequestResponse { message, code })).into_response()
    }
}
