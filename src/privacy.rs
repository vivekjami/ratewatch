use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DataDeletionRequest {
    pub user_id: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataDeletionResponse {
    pub success: bool,
    pub message: String,
    pub deleted_keys: u64,
}

pub struct PrivacyManager {
    redis: Client,
}

impl PrivacyManager {
    pub fn new(redis: Client) -> Self {
        Self { redis }
    }

    pub async fn delete_user_data(&self, user_id: &str) -> anyhow::Result<DataDeletionResponse> {
        let mut conn = self.redis.get_async_connection().await?;

        // Find all keys for this user using pattern matching
        let pattern = format!("rate_limit:{user_id}:*");
        let keys: Vec<String> = conn.keys(pattern).await?;

        let deleted_count = keys.len() as u64;

        if !keys.is_empty() {
            let _: () = conn.del(keys).await?;
            tracing::info!("Deleted {} keys for user: {}", deleted_count, user_id);
        } else {
            tracing::info!("No data found for user: {}", user_id);
        }

        Ok(DataDeletionResponse {
            success: true,
            message: format!("Successfully deleted data for user {user_id}"),
            deleted_keys: deleted_count,
        })
    }

    #[allow(dead_code)]
    pub async fn set_auto_deletion(&self, key: &str, ttl_seconds: i64) -> anyhow::Result<()> {
        let mut conn = self.redis.get_async_connection().await?;
        let _: () = conn.expire(key, ttl_seconds).await?;
        tracing::debug!("Set TTL of {} seconds for key: {}", ttl_seconds, key);
        Ok(())
    }

    pub async fn get_user_data_summary(&self, user_id: &str) -> anyhow::Result<UserDataSummary> {
        let mut conn = self.redis.get_async_connection().await?;

        let pattern = format!("rate_limit:{user_id}:*");
        let keys: Vec<String> = conn.keys(pattern).await?;

        let mut total_requests = 0u64;
        let mut active_windows = 0u64;

        for key in &keys {
            if let Ok(count) = conn.get::<_, u64>(key).await {
                total_requests += count;
                active_windows += 1;
            }
        }

        Ok(UserDataSummary {
            user_id: user_id.to_string(),
            total_keys: keys.len() as u64,
            total_requests,
            active_windows,
            data_retention_days: 30, // Default retention period
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserDataSummary {
    pub user_id: String,
    pub total_keys: u64,
    pub total_requests: u64,
    pub active_windows: u64,
    pub data_retention_days: u64,
}
