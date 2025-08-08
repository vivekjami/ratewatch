mod api;
mod rate_limiter;

use anyhow::Result;
use dotenvy::dotenv;
use std::{env, sync::Arc};
use tokio::net::TcpListener;
use tracing_subscriber;

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
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()?;
    
    // Initialize rate limiter
    let rate_limiter = Arc::new(rate_limiter::RateLimiter::new(&redis_url)?);
    
    // Create router
    let app = api::create_router(rate_limiter);
    
    // Start server
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("Server starting on port {}", port);
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
