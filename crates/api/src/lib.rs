pub mod errors;
pub mod routes;
pub mod state;

use axum::{
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use std::time::Duration;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/positions",         post(routes::positions::open_position))
        .route("/positions",         get(routes::positions::list_positions))
        .route("/positions/:id",     get(routes::positions::get_position))
        .route("/positions/:id/close", post(routes::positions::close_position))
        // Authentication: validate Bearer token against config.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .route("/health", get(routes::health::health))
        .nest("/v1", api_routes)
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match token {
        Some(t) if t == state.config.api.api_key => next.run(req).await,
        _ => (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({"error": {"code": "unauthorized"}})),
        )
            .into_response(),
    }
}
