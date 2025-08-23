mod analytics;
mod api;
mod audit;
mod auth;
mod config;
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
use config::ConfigManager;
use health::HealthCheckManager;
use privacy::PrivacyManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv().ok();

    // Initialize enterprise configuration management
    tracing::info!("🔧 Initializing enterprise configuration management...");
    let config_manager = ConfigManager::new().await?;
    let enterprise_config = config_manager.get_config().await;

    // Initialize structured logging based on configuration
    let log_level = enterprise_config.observability.logging.level.as_str();
    let log_format = &enterprise_config.observability.logging.format;
    
    if log_format == "json" && enterprise_config.observability.logging.structured {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(log_level)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(log_level)
            .init();
    }

    tracing::info!("✅ Enterprise configuration loaded and validated");

    // Extract configuration values
    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let port = enterprise_config.server.port;
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

    // Initialize audit system
    tracing::info!("🔍 Initializing enterprise audit system...");
    let audit_signing_key = env::var("AUDIT_SIGNING_KEY").unwrap_or_else(|_| {
        tracing::warn!("Using default AUDIT_SIGNING_KEY - change this in production!");
        "change-this-audit-signing-key-in-production-must-be-at-least-32-chars".to_string()
    });
    
    let audit_logger = audit::initialize_audit_system(
        "redis", // Use Redis for audit storage
        Some(redis::Client::open(redis_url.as_str())?),
        None,
        &audit_signing_key,
    ).await?;
    
    tracing::info!("✅ Enterprise audit system initialized");

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
        audit_logger,
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
