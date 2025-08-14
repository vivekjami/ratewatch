mod analytics;
mod api;
mod auth;
mod health;
mod metrics;
mod privacy;
mod rate_limiter;

use anyhow::Result;
use dotenvy::dotenv;
use std::{env, sync::Arc};
use tokio::net::TcpListener;

use analytics::AnalyticsManager;
use auth::ApiKeyValidator;
use health::HealthCheckManager;
use privacy::PrivacyManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv().ok();

    // Initialize logging
    tracing_subscriber::fmt::init();

    // Get configuration from environment
    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse::<u16>()?;
    let api_key_secret = env::var("API_KEY_SECRET").unwrap_or_else(|_| {
        tracing::warn!("Using default API_KEY_SECRET - change this in production!");
        "change-this-in-production".to_string()
    });

    // Initialize rate limiter
    let rate_limiter = Arc::new(rate_limiter::RateLimiter::new(&redis_url)?);

    // Initialize health check manager
    let health_manager = Arc::new(HealthCheckManager::new(rate_limiter.clone()));

    // Perform startup health validation
    tracing::info!("🔍 Performing startup health validation...");
    match health_manager.validate_startup_dependencies().await {
        Ok(_) => {
            tracing::info!("✅ Startup health validation passed");
        }
        Err(e) => {
            tracing::error!("❌ Startup health validation failed: {}", e);
            tracing::error!("💡 Check Redis connectivity and configuration");
            return Err(e);
        }
    }

    // Initialize security components
    let api_key_validator = Arc::new(ApiKeyValidator::new(api_key_secret));
    let privacy_manager = Arc::new(PrivacyManager::new(redis::Client::open(
        redis_url.as_str(),
    )?));
    let analytics_manager = Arc::new(AnalyticsManager::new(redis::Client::open(
        redis_url.as_str(),
    )?));

    // Create secure router
    let app = api::create_secure_router(
        rate_limiter,
        api_key_validator,
        privacy_manager,
        analytics_manager,
        health_manager,
    );

    // Start server
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!(
        "🚀 RateWatch server starting on port {} with security enabled",
        port
    );
    tracing::info!("🔒 Security features: API key auth, GDPR compliance, secure headers");

    axum::serve(listener, app).await?;

    Ok(())
}
