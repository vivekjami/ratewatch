use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::{Html, Json},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::analytics::AnalyticsManager;
use crate::auth::{auth_middleware, ApiKeyValidator};
use crate::health::HealthCheckManager;
use crate::metrics;
use crate::privacy::{DataDeletionRequest, PrivacyManager};
use crate::rate_limiter::{RateLimitRequest, RateLimiter};

pub struct AppState {
    pub rate_limiter: Arc<RateLimiter>,
    pub analytics: Arc<AnalyticsManager>,
    pub privacy: Arc<PrivacyManager>,
    pub health: Arc<HealthCheckManager>,
}

pub fn create_secure_router(
    rate_limiter: Arc<RateLimiter>,
    api_key_validator: Arc<ApiKeyValidator>,
    privacy_manager: Arc<PrivacyManager>,
    analytics: Arc<AnalyticsManager>,
    health_manager: Arc<HealthCheckManager>,
) -> Router {
    let app_state = Arc::new(AppState {
        rate_limiter,
        analytics: analytics.clone(),
        privacy: privacy_manager,
        health: health_manager,
    });

    // Protected routes that require authentication
    let protected_routes = Router::new()
        .route("/v1/check", post(check_rate_limit))
        .route("/v1/privacy/delete", post(delete_user_data))
        .route("/v1/privacy/summary", post(get_user_data_summary))
        .layer(middleware::from_fn_with_state(
            api_key_validator.clone(),
            auth_middleware,
        ))
        .with_state(app_state.clone());

    // Analytics routes (also protected)
    let analytics_routes = crate::analytics::create_analytics_router(analytics).layer(
        middleware::from_fn_with_state(api_key_validator, auth_middleware),
    );

    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/", get(serve_dashboard))
        .route("/dashboard", get(serve_dashboard))
        .route("/health", get(health_check))
        .route("/health/detailed", get(detailed_health_check))
        .route("/health/ready", get(readiness_check))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(app_state);

    // Combine routes and apply security middleware
    Router::new()
        .merge(protected_routes)
        .merge(analytics_routes)
        .merge(public_routes)
        .merge(metrics::create_metrics_router())
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
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods(Any)
                        .allow_headers(Any),
                ),
        )
}

async fn serve_dashboard() -> Html<String> {
    match tokio::fs::read_to_string("static/dashboard.html").await {
        Ok(content) => Html(content),
        Err(_) => Html("<html><body><h1>Dashboard not found</h1><p>Please ensure static/dashboard.html exists</p></body></html>".to_string()),
    }
}

async fn check_rate_limit(
    State(app_state): State<Arc<AppState>>,
    Json(payload): Json<RateLimitRequest>,
) -> Result<Json<Value>, StatusCode> {
    let start_time = std::time::Instant::now();

    // Record request
    metrics::REQUEST_TOTAL.inc();

    match app_state.rate_limiter.check(payload.clone()).await {
        Ok(response) => {
            // Record metrics
            let duration = start_time.elapsed().as_secs_f64();
            metrics::REQUEST_DURATION.observe(duration);

            if response.allowed {
                metrics::RATE_LIMIT_HITS.inc();
            } else {
                metrics::RATE_LIMIT_MISSES.inc();
            }

            // Record analytics
            let _ = app_state
                .analytics
                .record_request(&payload.key, response.allowed, payload.window)
                .await;

            // Log activity if rate limited
            if !response.allowed {
                let _ = app_state
                    .analytics
                    .log_activity(
                        &format!("Rate limit exceeded for key: {}", payload.key),
                        "warning",
                        Some(&payload.key),
                    )
                    .await;
            }

            tracing::debug!("Rate limit check completed successfully");
            Ok(Json(json!(response)))
        }
        Err(err) => {
            let _ = app_state
                .analytics
                .log_activity(
                    &format!("Rate limit check failed: {err}"),
                    "error",
                    Some(&payload.key),
                )
                .await;

            tracing::error!("Rate limit check failed: {}", err);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn delete_user_data(
    State(app_state): State<Arc<AppState>>,
    Json(payload): Json<DataDeletionRequest>,
) -> Result<Json<Value>, StatusCode> {
    match app_state.privacy.delete_user_data(&payload.user_id).await {
        Ok(response) => {
            let _ = app_state
                .analytics
                .log_activity(
                    &format!(
                        "User data deleted for: {} (reason: {})",
                        payload.user_id, payload.reason
                    ),
                    "info",
                    Some(&payload.user_id),
                )
                .await;

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
    State(app_state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<Value>, StatusCode> {
    let user_id = payload
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    match app_state.privacy.get_user_data_summary(user_id).await {
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

async fn health_check(
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    match app_state.health.quick_health_check().await {
        Ok(status) => {
            let response = json!({
                "status": match status {
                    crate::health::ServiceStatus::Healthy => "ok",
                    crate::health::ServiceStatus::Degraded => "degraded",
                    crate::health::ServiceStatus::Unhealthy => "unhealthy",
                    crate::health::ServiceStatus::Starting => "starting",
                },
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "version": env!("CARGO_PKG_VERSION")
            });

            match status {
                crate::health::ServiceStatus::Healthy | crate::health::ServiceStatus::Degraded => {
                    Ok(Json(response))
                }
                _ => Err(StatusCode::SERVICE_UNAVAILABLE),
            }
        }
        Err(e) => {
            tracing::error!("Health check failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn detailed_health_check(
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    match app_state.health.check_startup_health().await {
        Ok(health_status) => {
            let response = json!({
                "status": match health_status.status {
                    crate::health::ServiceStatus::Healthy => "ok",
                    crate::health::ServiceStatus::Degraded => "degraded",
                    crate::health::ServiceStatus::Unhealthy => "unhealthy",
                    crate::health::ServiceStatus::Starting => "starting",
                },
                "timestamp": health_status.timestamp.to_rfc3339(),
                "version": health_status.version,
                "startup_time": health_status.startup_time.to_rfc3339(),
                "uptime_seconds": health_status.metrics.uptime_seconds,
                "dependencies": health_status.dependencies,
                "metrics": health_status.metrics
            });

            match health_status.status {
                crate::health::ServiceStatus::Healthy | crate::health::ServiceStatus::Degraded => {
                    Ok(Json(response))
                }
                _ => Err(StatusCode::SERVICE_UNAVAILABLE),
            }
        }
        Err(e) => {
            tracing::error!("Detailed health check failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn readiness_check(
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<Value>, StatusCode> {
    match app_state.health.is_ready().await {
        Ok(true) => {
            let response = json!({
                "status": "ready",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "version": env!("CARGO_PKG_VERSION")
            });
            Ok(Json(response))
        }
        Ok(false) => {
            let _response = json!({
                "status": "not_ready",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "version": env!("CARGO_PKG_VERSION")
            });
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
        Err(e) => {
            tracing::error!("Readiness check failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
