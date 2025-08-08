use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::{SystemTime, UNIX_EPOCH}};

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyticsQuery {
    pub key: Option<String>,
    pub window: Option<String>, // 1h, 6h, 24h, 7d
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyMetric {
    pub key: String,
    pub count: u64,
    pub success_rate: f64,
    pub last_seen: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActivityLog {
    pub timestamp: String,
    pub message: String,
    pub level: String, // info, warning, error
    pub key: Option<String>,
}

pub struct AnalyticsManager {
    redis: Client,
}

impl AnalyticsManager {
    pub fn new(redis: Client) -> Self {
        Self { redis }
    }
    
    /// Record a rate limit check for analytics
    pub async fn record_request(&self, key: &str, allowed: bool, _window: u64) -> anyhow::Result<()> {
        let mut conn = self.redis.get_async_connection().await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        
        // Record per-minute statistics
        let minute_key = format!("analytics:minute:{}:{}", now / 60, key);
        let _: () = conn.incr(&minute_key, 1).await?;
        let _: () = conn.expire(&minute_key, 3600).await?; // Keep for 1 hour
        
        // Record success/failure stats
        let status = if allowed { "allowed" } else { "denied" };
        let status_key = format!("analytics:status:{}:{}", status, now / 60);
        let _: () = conn.incr(&status_key, 1).await?;
        let _: () = conn.expire(&status_key, 86400).await?; // Keep for 24 hours
        
        // Update key statistics
        let key_stats = format!("analytics:key_stats:{}", key);
        let _: () = conn.hincr(&key_stats, "total_requests", 1).await?;
        if allowed {
            let _: () = conn.hincr(&key_stats, "allowed_requests", 1).await?;
        }
        let _: () = conn.hset(&key_stats, "last_seen", now).await?;
        let _: () = conn.expire(&key_stats, 2592000).await?; // Keep for 30 days
        
        // Record daily totals
        let daily_key = format!("analytics:daily:{}", now / 86400);
        let _: () = conn.incr(&daily_key, 1).await?;
        let _: () = conn.expire(&daily_key, 2592000).await?; // Keep for 30 days
        
        Ok(())
    }
    
    /// Log an activity event
    pub async fn log_activity(&self, message: &str, level: &str, key: Option<&str>) -> anyhow::Result<()> {
        let mut conn = self.redis.get_async_connection().await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        
        let log_entry = ActivityLog {
            timestamp: chrono::DateTime::from_timestamp(now as i64, 0)
                .unwrap_or_default()
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string(),
            message: message.to_string(),
            level: level.to_string(),
            key: key.map(|s| s.to_string()),
        };
        
        let log_json = serde_json::to_string(&log_entry)?;
        let log_key = "analytics:activity_log";
        
        // Add to list (most recent first)
        let _: () = conn.lpush(&log_key, &log_json).await?;
        // Keep only last 100 entries
        let _: () = conn.ltrim(&log_key, 0, 99).await?;
        let _: () = conn.expire(&log_key, 86400).await?; // Expire in 24 hours
        
        Ok(())
    }
    
    /// Get overall statistics
    pub async fn get_stats(&self) -> anyhow::Result<Value> {
        let mut conn = self.redis.get_async_connection().await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        
        // Get today's total requests
        let today_key = format!("analytics:daily:{}", now / 86400);
        let today_requests: u64 = conn.get(&today_key).await.unwrap_or(0);
        
        // Get yesterday's requests for comparison
        let yesterday_key = format!("analytics:daily:{}", (now / 86400) - 1);
        let yesterday_requests: u64 = conn.get(&yesterday_key).await.unwrap_or(1);
        
        // Calculate success rate for the last hour
        let hour_start = now - 3600;
        let mut total_allowed = 0u64;
        let mut total_denied = 0u64;
        
        for minute in (hour_start / 60)..=(now / 60) {
            let allowed_key = format!("analytics:status:allowed:{}", minute);
            let denied_key = format!("analytics:status:denied:{}", minute);
            
            total_allowed += conn.get(&allowed_key).await.unwrap_or(0);
            total_denied += conn.get(&denied_key).await.unwrap_or(0);
        }
        
        let total_requests = total_allowed + total_denied;
        let success_rate = if total_requests > 0 {
            (total_allowed as f64 / total_requests as f64) * 100.0
        } else {
            100.0
        };
        
        let requests_change = if yesterday_requests > 0 {
            ((today_requests as f64 - yesterday_requests as f64) / yesterday_requests as f64) * 100.0
        } else {
            0.0
        };
        
        Ok(json!({
            "total_requests_today": today_requests,
            "requests_change": requests_change.round(),
            "success_rate": success_rate,
            "avg_response_time": 245, // Mock value - implement real response time tracking
            "total_requests_hour": total_requests,
            "allowed_requests_hour": total_allowed,
            "denied_requests_hour": total_denied,
            "uptime": "99.9%"
        }))
    }
    
    /// Get top rate limited keys
    pub async fn get_top_keys(&self, limit: u32) -> anyhow::Result<Value> {
        let mut conn = self.redis.get_async_connection().await?;
        
        // Get all key stats
        let pattern = "analytics:key_stats:*";
        let keys: Vec<String> = conn.keys(pattern).await?;
        
        let mut key_metrics = Vec::new();
        
        for key in keys.iter().take(limit as usize * 2) { // Get more than needed in case some are empty
            if let Ok(stats) = conn.hgetall::<_, HashMap<String, String>>(key).await {
                let clean_key = key.strip_prefix("analytics:key_stats:").unwrap_or(key);
                let total_requests: u64 = stats.get("total_requests")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let allowed_requests: u64 = stats.get("allowed_requests")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let last_seen: u64 = stats.get("last_seen")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                
                if total_requests > 0 {
                    let success_rate = (allowed_requests as f64 / total_requests as f64) * 100.0;
                    let last_seen_str = chrono::DateTime::from_timestamp(last_seen as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                        .unwrap_or_else(|| "Unknown".to_string());
                    
                    key_metrics.push(KeyMetric {
                        key: clean_key.to_string(),
                        count: total_requests,
                        success_rate,
                        last_seen: last_seen_str,
                    });
                }
            }
        }
        
        // Sort by request count and take top N
        key_metrics.sort_by(|a, b| b.count.cmp(&a.count));
        key_metrics.truncate(limit as usize);
        
        Ok(json!({
            "top_keys": key_metrics
        }))
    }
    
    /// Get recent activity logs
    pub async fn get_recent_activity(&self, limit: u32) -> anyhow::Result<Value> {
        let mut conn = self.redis.get_async_connection().await?;
        
        let log_key = "analytics:activity_log";
        let logs: Vec<String> = conn.lrange(&log_key, 0, (limit as isize) - 1).await?;
        
        let mut activity_logs = Vec::new();
        for log_json in logs {
            if let Ok(log) = serde_json::from_str::<ActivityLog>(&log_json) {
                activity_logs.push(log);
            }
        }
        
        Ok(json!({
            "logs": activity_logs
        }))
    }
    
    /// Get request rate data for charts
    pub async fn get_request_rate_data(&self, window: &str) -> anyhow::Result<Value> {
        let mut conn = self.redis.get_async_connection().await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        
        let (data_points, interval) = match window {
            "1h" => (60, 60),     // 60 minutes, 1-minute intervals
            "6h" => (72, 300),    // 72 points, 5-minute intervals
            "24h" => (24, 3600),  // 24 hours, 1-hour intervals
            "7d" => (7, 86400),   // 7 days, 1-day intervals
            _ => (60, 60),        // default to 1 hour
        };
        
        let mut allowed_data = Vec::new();
        let mut denied_data = Vec::new();
        let mut labels = Vec::new();
        
        for i in 0..data_points {
            let time_point = now - (i * interval);
            let time_bucket = time_point / interval;
            
            let allowed_key = format!("analytics:status:allowed:{}", time_bucket);
            let denied_key = format!("analytics:status:denied:{}", time_bucket);
            
            let allowed: u64 = conn.get(&allowed_key).await.unwrap_or(0);
            let denied: u64 = conn.get(&denied_key).await.unwrap_or(0);
            
            allowed_data.insert(0, allowed);
            denied_data.insert(0, denied);
            
            // Generate label based on time point
            let dt = chrono::DateTime::from_timestamp(time_point as i64, 0)
                .unwrap_or_default();
            let label = match window {
                "1h" | "6h" => dt.format("%H:%M").to_string(),
                "24h" => dt.format("%H:00").to_string(),
                "7d" => dt.format("%m/%d").to_string(),
                _ => dt.format("%H:%M").to_string(),
            };
            labels.insert(0, label);
        }
        
        Ok(json!({
            "labels": labels,
            "allowed_data": allowed_data,
            "denied_data": denied_data
        }))
    }
}

pub fn create_analytics_router(analytics: Arc<AnalyticsManager>) -> Router {
    Router::new()
        .route("/v1/analytics/stats", get(get_stats))
        .route("/v1/analytics/top-keys", get(get_top_keys))
        .route("/v1/analytics/recent-activity", get(get_recent_activity))
        .route("/v1/analytics/request-rate", get(get_request_rate))
        .with_state(analytics)
}

async fn get_stats(
    State(analytics): State<Arc<AnalyticsManager>>,
) -> Result<Json<Value>, StatusCode> {
    match analytics.get_stats().await {
        Ok(stats) => Ok(Json(stats)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_top_keys(
    State(analytics): State<Arc<AnalyticsManager>>,
    Query(params): Query<AnalyticsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.limit.unwrap_or(10);
    match analytics.get_top_keys(limit).await {
        Ok(keys) => Ok(Json(keys)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_recent_activity(
    State(analytics): State<Arc<AnalyticsManager>>,
    Query(params): Query<AnalyticsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let limit = params.limit.unwrap_or(10);
    match analytics.get_recent_activity(limit).await {
        Ok(logs) => Ok(Json(logs)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_request_rate(
    State(analytics): State<Arc<AnalyticsManager>>,
    Query(params): Query<AnalyticsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let window = params.window.as_deref().unwrap_or("1h");
    match analytics.get_request_rate_data(window).await {
        Ok(data) => Ok(Json(data)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
