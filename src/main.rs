mod api;
mod auth;
mod privacy;
mod rate_limiter;

use anyhow::Result;
use dotenvy::dotenv;
use std::{env, sync::Arc};
use tokio::net::TcpListener;
use tracing_subscriber;

use auth::ApiKeyValidator;
use privacy::PrivacyManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv().ok();
    
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Get configuration from environment
    let redis_url = env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse::<u16>()?;
    let api_key_secret = env::var("API_KEY_SECRET")
        .unwrap_or_else(|_| {
            tracing::warn!("Using default API_KEY_SECRET - change this in production!");
            "change-this-in-production".to_string()
        });
    
    // Initialize rate limiter
    let rate_limiter = Arc::new(rate_limiter::RateLimiter::new(&redis_url)?);
    
    // Initialize security components
    let api_key_validator = Arc::new(ApiKeyValidator::new(api_key_secret));
    let privacy_manager = Arc::new(PrivacyManager::new(
        redis::Client::open(redis_url.as_str())?
    ));
    
    // Create secure router
    let app = api::create_secure_router(
        rate_limiter,
        api_key_validator,
        privacy_manager,
    );
    
    // Start server
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("🚀 RateWatch server starting on port {} with security enabled", port);
    tracing::info!("🔒 Security features: API key auth, GDPR compliance, secure headers");
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
