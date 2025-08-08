use redis::{Client, AsyncCommands};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
pub struct RateLimitRequest {
    pub key: String,
    pub limit: u64,
    pub window: u64,
    pub cost: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RateLimitResponse {
    pub allowed: bool,
    pub remaining: u64,
    pub reset_in: u64,
    pub retry_after: Option<u64>,
}

pub struct RateLimiter {
    redis: Client,
}

impl RateLimiter {
    pub fn new(redis_url: &str) -> anyhow::Result<Self> {
        let redis = Client::open(redis_url)?;
        Ok(Self { redis })
    }

    pub async fn check(&self, req: RateLimitRequest) -> anyhow::Result<RateLimitResponse> {
        let mut conn = self.redis.get_async_connection().await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        
        let window_start = now - (now % req.window);
        let redis_key = format!("rate_limit:{}:{}", req.key, window_start);
        
        // Get current count
        let current: u64 = conn.get(&redis_key).await.unwrap_or(0);
        
        if current + req.cost <= req.limit {
            // Allow request and increment
            let _: () = conn.incr(&redis_key, req.cost).await?;
            let _: () = conn.expire(&redis_key, req.window as i64).await?;
            
            Ok(RateLimitResponse {
                allowed: true,
                remaining: req.limit - (current + req.cost),
                reset_in: req.window - (now % req.window),
                retry_after: None,
            })
        } else {
            // Deny request
            Ok(RateLimitResponse {
                allowed: false,
                remaining: 0,
                reset_in: req.window - (now % req.window),
                retry_after: Some(req.window - (now % req.window)),
            })
        }
    }
}
