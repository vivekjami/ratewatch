use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
    set_header::SetResponseHeaderLayer,
};

use crate::auth::{auth_middleware, ApiKeyValidator};
use crate::privacy::{DataDeletionRequest, PrivacyManager};
use crate::rate_limiter::{RateLimiter, RateLimitRequest};

pub fn create_router(rate_limiter: Arc<RateLimiter>) -> Router {
    // Create a dummy privacy manager for backwards compatibility
    let dummy_privacy_manager = Arc::new(PrivacyManager::new(
        redis::Client::open("redis://127.0.0.1:6379").unwrap()
    ));
    
    Router::new()
        .route("/v1/check", post(check_rate_limit))
        .route("/health", get(health_check_simple))
        .with_state((rate_limiter, dummy_privacy_manager))
}

pub fn create_secure_router(
    rate_limiter: Arc<RateLimiter>,
    api_key_validator: Arc<ApiKeyValidator>,
    privacy_manager: Arc<PrivacyManager>,
) -> Router {
    // Protected routes that require authentication
    let protected_routes = Router::new()
        .route("/v1/check", post(check_rate_limit))
        .route("/v1/privacy/delete", post(delete_user_data))
        .route("/v1/privacy/summary", post(get_user_data_summary))
        .layer(middleware::from_fn_with_state(
            api_key_validator,
            auth_middleware,
        ))
        .with_state((rate_limiter.clone(), privacy_manager.clone()));

    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/health/detailed", get(detailed_health_check))
        .with_state((rate_limiter.clone(), privacy_manager.clone()));

    // Combine routes and apply security middleware
    Router::new()
        .merge(protected_routes)
        .merge(public_routes)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                ))
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::X_FRAME_OPTIONS,
                    HeaderValue::from_static("DENY"),
                ))
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::STRICT_TRANSPORT_SECURITY,
                    HeaderValue::from_static("max-age=31536000; includeSubDomains"),
                ))
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::X_XSS_PROTECTION,
                    HeaderValue::from_static("1; mode=block"),
                ))
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::REFERRER_POLICY,
                    HeaderValue::from_static("strict-origin-when-cross-origin"),
                ))
                .layer(CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any))
        )
}

async fn check_rate_limit(
    State((rate_limiter, _privacy_manager)): State<(Arc<RateLimiter>, Arc<PrivacyManager>)>,
    Json(payload): Json<RateLimitRequest>,
) -> Result<Json<Value>, StatusCode> {
    match rate_limiter.check(payload).await {
        Ok(response) => {
            tracing::debug!("Rate limit check completed successfully");
            Ok(Json(json!(response)))
        }
        Err(err) => {
            tracing::error!("Rate limit check failed: {}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn delete_user_data(
    State((_rate_limiter, privacy_manager)): State<(Arc<RateLimiter>, Arc<PrivacyManager>)>,
    Json(payload): Json<DataDeletionRequest>,
) -> Result<Json<Value>, StatusCode> {
    match privacy_manager.delete_user_data(&payload.user_id).await {
        Ok(response) => {
            tracing::info!("Data deletion completed for user: {}", payload.user_id);
            Ok(Json(json!(response)))
        }
        Err(err) => {
            tracing::error!("Data deletion failed: {}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_user_data_summary(
    State((_rate_limiter, privacy_manager)): State<(Arc<RateLimiter>, Arc<PrivacyManager>)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Value>, StatusCode> {
    let user_id = payload.get("user_id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    match privacy_manager.get_user_data_summary(user_id).await {
        Ok(summary) => {
            tracing::debug!("Data summary retrieved for user: {}", user_id);
            Ok(Json(json!(summary)))
        }
        Err(err) => {
            tracing::error!("Failed to get data summary: {}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn health_check_simple() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn detailed_health_check(
    State((rate_limiter, _privacy_manager)): State<(Arc<RateLimiter>, Arc<PrivacyManager>)>,
) -> Result<Json<Value>, StatusCode> {
    // Test Redis connectivity
    let redis_status = match rate_limiter.health_check().await {
        Ok(_) => "healthy",
        Err(_) => "unhealthy",
    };

    let health_data = json!({
        "status": if redis_status == "healthy" { "ok" } else { "degraded" },
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "components": {
            "redis": {
                "status": redis_status,
                "response_time_ms": 0 // Could add actual timing here
            },
            "api": {
                "status": "healthy"
            }
        }
    });

    if redis_status == "healthy" {
        Ok(Json(health_data))
    } else {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}
