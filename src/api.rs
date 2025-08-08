use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::rate_limiter::{RateLimiter, RateLimitRequest};

pub fn create_router(rate_limiter: Arc<RateLimiter>) -> Router {
    Router::new()
        .route("/v1/check", post(check_rate_limit))
        .route("/health", axum::routing::get(health_check))
        .with_state(rate_limiter)
}

async fn check_rate_limit(
    State(rate_limiter): State<Arc<RateLimiter>>,
    Json(payload): Json<RateLimitRequest>,
) -> Result<Json<Value>, StatusCode> {
    match rate_limiter.check(payload).await {
        Ok(response) => Ok(Json(json!(response))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}
