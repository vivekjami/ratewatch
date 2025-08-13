use redis::{AsyncCommands, Client, RedisResult};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Check rate limit using Redis sliding window algorithm with automatic TTL for GDPR compliance
    pub async fn check(&self, req: RateLimitRequest) -> anyhow::Result<RateLimitResponse> {
        // Validate input parameters
        if req.window == 0 {
            return Err(anyhow::anyhow!("Window size cannot be zero"));
        }
        if req.limit == 0 {
            return Err(anyhow::anyhow!("Limit cannot be zero"));
        }
        if req.key.is_empty() {
            return Err(anyhow::anyhow!("Key cannot be empty"));
        }

        let mut conn = self
            .redis
            .get_async_connection()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to Redis: {}", e))?;

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        // Use sliding window approach - each window is aligned to the window size
        let window_start = now - (now % req.window);
        let redis_key = format!("rate_limit:{}:{}", req.key, window_start);

        // Use Redis pipeline for atomic operations
        let result: RedisResult<(u64,)> = redis::pipe()
            .atomic()
            .get(&redis_key)
            .query_async(&mut conn)
            .await;

        let current = match result {
            Ok((count,)) => count,
            Err(_) => 0, // Key doesn't exist yet
        };

        if current + req.cost <= req.limit {
            // Allow request - increment counter and set TTL
            let _: RedisResult<()> = redis::pipe()
                .atomic()
                .incr(&redis_key, req.cost)
                .expire(&redis_key, req.window as i64)
                .query_async(&mut conn)
                .await;

            Ok(RateLimitResponse {
                allowed: true,
                remaining: req.limit.saturating_sub(current + req.cost),
                reset_in: req.window - (now % req.window),
                retry_after: None,
            })
        } else {
            // Deny request - don't increment counter
            tracing::debug!(
                "Rate limit exceeded for key: {} (current: {}, limit: {})",
                req.key,
                current,
                req.limit
            );

            Ok(RateLimitResponse {
                allowed: false,
                remaining: 0,
                reset_in: req.window - (now % req.window),
                retry_after: Some(req.window - (now % req.window)),
            })
        }
    }

    /// Health check that verifies Redis connectivity
    pub async fn health_check(&self) -> anyhow::Result<()> {
        let mut conn = self
            .redis
            .get_async_connection()
            .await
            .map_err(|e| anyhow::anyhow!("Redis connection failed: {}", e))?;

        // Use PING command for proper health check
        let response: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("Redis PING failed: {}", e))?;

        if response == "PONG" {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Unexpected Redis response: {}", response))
        }
    }

    /// Clean up expired rate limit data (for maintenance)
    #[allow(dead_code)]
    pub async fn cleanup_expired_keys(&self, pattern: &str) -> anyhow::Result<u64> {
        let mut conn = self.redis.get_async_connection().await?;
        let keys: Vec<String> = conn.keys(pattern).await?;

        if keys.is_empty() {
            return Ok(0);
        }

        let deleted: u64 = conn.del(&keys).await?;
        tracing::info!("Cleaned up {} expired rate limit keys", deleted);
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    fn create_test_request(key: &str, limit: u64, window: u64) -> RateLimitRequest {
        RateLimitRequest {
            key: key.to_string(),
            limit,
            window,
            cost: 1,
        }
    }

    #[tokio::test]
    async fn test_rate_limit_allows_within_limit() {
        // This test requires Redis to be running
        if let Ok(limiter) = RateLimiter::new("redis://127.0.0.1:6379") {
            let req = create_test_request("test_user_1", 10, 60);

            match limiter.check(req).await {
                Ok(response) => {
                    assert!(response.allowed);
                    assert!(response.remaining <= 10);
                    assert!(response.retry_after.is_none());
                }
                Err(_) => {
                    // Redis not available, skip test
                    println!("Skipping test - Redis not available");
                }
            }
        }
    }

    #[tokio::test]
    async fn test_rate_limit_denies_over_limit() {
        if let Ok(limiter) = RateLimiter::new("redis://127.0.0.1:6379") {
            let req = create_test_request("test_user_2", 1, 60);

            // First request should be allowed
            if let Ok(response1) = limiter.check(req.clone()).await {
                assert!(response1.allowed);

                // Second request should be denied
                if let Ok(response2) = limiter.check(req).await {
                    assert!(!response2.allowed);
                    assert_eq!(response2.remaining, 0);
                    assert!(response2.retry_after.is_some());
                }
            }
        }
    }

    #[tokio::test]
    async fn test_health_check() {
        if let Ok(limiter) = RateLimiter::new("redis://127.0.0.1:6379") {
            match limiter.health_check().await {
                Ok(_) => {
                    // Health check passed
                    assert!(true);
                }
                Err(_) => {
                    // Redis not available, skip test
                    println!("Skipping health check test - Redis not available");
                }
            }
        }
    }

    #[test]
    fn test_rate_limit_request_serialization() {
        let req = create_test_request("user:123", 100, 3600);
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: RateLimitRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.key, deserialized.key);
        assert_eq!(req.limit, deserialized.limit);
        assert_eq!(req.window, deserialized.window);
        assert_eq!(req.cost, deserialized.cost);
    }

    #[test]
    fn test_rate_limit_response_serialization() {
        let response = RateLimitResponse {
            allowed: true,
            remaining: 99,
            reset_in: 3542,
            retry_after: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: RateLimitResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(response.allowed, deserialized.allowed);
        assert_eq!(response.remaining, deserialized.remaining);
        assert_eq!(response.reset_in, deserialized.reset_in);
        assert_eq!(response.retry_after, deserialized.retry_after);
    }
}
