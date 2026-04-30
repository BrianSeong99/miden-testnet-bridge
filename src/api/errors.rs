use axum::{
    Json,
    extract::rejection::{JsonRejection, QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::types::BadRequestResponse;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{message}")]
    BadRequest { message: String },
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    pub fn from_json_rejection(rejection: JsonRejection) -> Self {
        Self::bad_request(rejection.body_text())
    }

    pub fn from_query_rejection(rejection: QueryRejection) -> Self {
        Self::bad_request(rejection.body_text())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest { message } => (StatusCode::BAD_REQUEST, message),
            Self::NotFound { message } => (StatusCode::NOT_FOUND, message),
            Self::Internal { message } => (StatusCode::INTERNAL_SERVER_ERROR, message),
        };

        (status, Json(BadRequestResponse { message })).into_response()
    }
}
