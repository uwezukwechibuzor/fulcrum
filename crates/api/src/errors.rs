use axum::{http::StatusCode, response::IntoResponse, Json};
use common::EngineError;
use serde_json::json;

pub struct ApiError(pub EngineError);

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code) = match &self.0 {
            EngineError::PositionNotFound(_)       => (StatusCode::NOT_FOUND, "not_found"),
            EngineError::ConcurrentModification(_) => (StatusCode::CONFLICT, "conflict"),
            EngineError::AlreadyTerminal(_)        => (StatusCode::CONFLICT, "already_terminal"),
            EngineError::IllegalTransition { .. }  => (StatusCode::UNPROCESSABLE_ENTITY, "illegal_transition"),
            EngineError::Rpc(_)                    => (StatusCode::BAD_GATEWAY, "rpc_error"),
            EngineError::ChannelClosed             => (StatusCode::SERVICE_UNAVAILABLE, "engine_unavailable"),
            _                                      => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        let body = Json(json!({
            "error": {
                "code":    code,
                "message": self.0.to_string(),
            }
        }));

        (status, body).into_response()
    }
}

impl From<EngineError> for ApiError {
    fn from(e: EngineError) -> Self {
        Self(e)
    }
}
