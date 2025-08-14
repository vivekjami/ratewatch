use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

use crate::rate_limiter::RateLimiter;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Starting,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DependencyHealth {
    pub name: String,
    pub status: ServiceStatus,
    pub latency_ms: Option<u64>,
    pub last_check: DateTime<Utc>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub uptime_seconds: u64,
    pub memory_usage_mb: Option<u64>,
    pub cpu_usage_percent: Option<f64>,
    pub active_connections: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: ServiceStatus,
    pub timestamp: DateTime<Utc>,
    pub version: String,
    pub dependencies: HashMap<String, DependencyHealth>,
    pub metrics: HealthMetrics,
    pub startup_time: DateTime<Utc>,
}

pub struct HealthCheckManager {
    rate_limiter: Arc<RateLimiter>,
    startup_time: Instant,
    startup_timestamp: DateTime<Utc>,
}

impl HealthCheckManager {
    pub fn new(rate_limiter: Arc<RateLimiter>) -> Self {
        Self {
            rate_limiter,
            startup_time: Instant::now(),
            startup_timestamp: Utc::now(),
        }
    }

    /// Perform comprehensive startup health validation
    pub async fn check_startup_health(&self) -> Result<HealthStatus> {
        tracing::info!("Starting comprehensive health check validation");

        let mut dependencies = HashMap::new();
        let mut overall_status = ServiceStatus::Healthy;

        // Check Redis dependency with timeout
        let redis_health = self.check_redis_dependency().await;
        if redis_health.status != ServiceStatus::Healthy {
            overall_status = ServiceStatus::Degraded;
        }
        dependencies.insert("redis".to_string(), redis_health);

        // Check internal API health
        let api_health = self.check_api_health().await;
        dependencies.insert("api".to_string(), api_health);

        // Check configuration validity
        let config_health = self.check_configuration().await;
        if config_health.status != ServiceStatus::Healthy {
            overall_status = ServiceStatus::Unhealthy;
        }
        dependencies.insert("configuration".to_string(), config_health);

        let health_status = HealthStatus {
            status: overall_status,
            timestamp: Utc::now(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            dependencies,
            metrics: self.get_health_metrics(),
            startup_time: self.startup_timestamp,
        };

        tracing::info!(
            "Health check completed with status: {:?}",
            health_status.status
        );
        Ok(health_status)
    }

    /// Check Redis dependency with detailed validation
    async fn check_redis_dependency(&self) -> DependencyHealth {
        let start_time = Instant::now();

        match timeout(Duration::from_secs(5), self.rate_limiter.health_check()).await {
            Ok(Ok(_)) => {
                let latency = start_time.elapsed().as_millis() as u64;
                tracing::debug!("Redis health check passed in {}ms", latency);

                DependencyHealth {
                    name: "redis".to_string(),
                    status: ServiceStatus::Healthy,
                    latency_ms: Some(latency),
                    last_check: Utc::now(),
                    error_message: None,
                }
            }
            Ok(Err(e)) => {
                tracing::error!("Redis health check failed: {}", e);
                DependencyHealth {
                    name: "redis".to_string(),
                    status: ServiceStatus::Unhealthy,
                    latency_ms: None,
                    last_check: Utc::now(),
                    error_message: Some(e.to_string()),
                }
            }
            Err(_) => {
                tracing::error!("Redis health check timed out after 5 seconds");
                DependencyHealth {
                    name: "redis".to_string(),
                    status: ServiceStatus::Unhealthy,
                    latency_ms: None,
                    last_check: Utc::now(),
                    error_message: Some("Health check timed out".to_string()),
                }
            }
        }
    }

    /// Check internal API health
    async fn check_api_health(&self) -> DependencyHealth {
        // For now, we assume API is healthy if we can create this health check
        // In a more complex system, this could check internal state, memory usage, etc.
        DependencyHealth {
            name: "api".to_string(),
            status: ServiceStatus::Healthy,
            latency_ms: Some(1),
            last_check: Utc::now(),
            error_message: None,
        }
    }

    /// Check configuration validity
    async fn check_configuration(&self) -> DependencyHealth {
        let mut errors = Vec::new();

        // Check required environment variables
        if std::env::var("REDIS_URL").is_err() {
            errors.push("REDIS_URL not configured");
        }

        let api_key_secret = std::env::var("API_KEY_SECRET").unwrap_or_default();
        if api_key_secret.len() < 32 {
            errors.push("API_KEY_SECRET is too short (minimum 32 characters)");
        }

        // Check port configuration
        if let Ok(port_str) = std::env::var("PORT") {
            match port_str.parse::<u16>() {
                Ok(port) => {
                    if port < 1024 {
                        errors.push("PORT is below 1024 (requires root privileges)");
                    }
                }
                Err(_) => {
                    errors.push("PORT is not a valid number");
                }
            }
        }

        // Check Redis URL format
        if let Ok(redis_url) = std::env::var("REDIS_URL") {
            if !redis_url.starts_with("redis://") && !redis_url.starts_with("rediss://") {
                errors.push("REDIS_URL must start with redis:// or rediss://");
            }
        }

        if errors.is_empty() {
            tracing::debug!("Configuration validation passed");
            DependencyHealth {
                name: "configuration".to_string(),
                status: ServiceStatus::Healthy,
                latency_ms: Some(1),
                last_check: Utc::now(),
                error_message: None,
            }
        } else {
            let error_message = errors.join(", ");
            tracing::error!("Configuration validation failed: {}", error_message);

            DependencyHealth {
                name: "configuration".to_string(),
                status: ServiceStatus::Unhealthy,
                latency_ms: Some(1),
                last_check: Utc::now(),
                error_message: Some(error_message),
            }
        }
    }

    /// Get current health metrics
    fn get_health_metrics(&self) -> HealthMetrics {
        let uptime_seconds = self.startup_time.elapsed().as_secs();

        HealthMetrics {
            uptime_seconds,
            memory_usage_mb: self.get_memory_usage(),
            cpu_usage_percent: None, // Could be implemented with system metrics
            active_connections: None, // Could be tracked by the server
        }
    }

    /// Get memory usage (simplified implementation)
    fn get_memory_usage(&self) -> Option<u64> {
        // This is a simplified implementation
        // In production, you might want to use a proper system metrics library
        None
    }

    /// Perform a quick health check (for frequent monitoring)
    pub async fn quick_health_check(&self) -> Result<ServiceStatus> {
        // Quick Redis ping with short timeout
        match timeout(Duration::from_secs(2), self.rate_limiter.health_check()).await {
            Ok(Ok(_)) => {
                tracing::debug!("Quick health check passed - Redis is healthy");
                Ok(ServiceStatus::Healthy)
            }
            Ok(Err(e)) => {
                tracing::warn!("Quick health check degraded - Redis error: {}", e);
                Ok(ServiceStatus::Degraded)
            }
            Err(_) => {
                tracing::error!("Quick health check failed - Redis timeout after 2 seconds");
                Ok(ServiceStatus::Unhealthy)
            }
        }
    }

    /// Validate that all dependencies are ready for service startup
    pub async fn validate_startup_dependencies(&self) -> Result<()> {
        tracing::info!("Validating startup dependencies...");

        let health_status = self.check_startup_health().await?;

        match health_status.status {
            ServiceStatus::Healthy => {
                tracing::info!("✅ All startup dependencies are healthy");
                Ok(())
            }
            ServiceStatus::Degraded => {
                tracing::warn!("⚠️ Some dependencies are degraded, but service can start");
                Ok(())
            }
            ServiceStatus::Unhealthy => {
                let unhealthy_deps: Vec<String> = health_status
                    .dependencies
                    .iter()
                    .filter(|(_, dep)| dep.status == ServiceStatus::Unhealthy)
                    .map(|(name, dep)| {
                        format!(
                            "{}: {}",
                            name,
                            dep.error_message.as_deref().unwrap_or("Unknown error")
                        )
                    })
                    .collect();

                let error_msg = format!(
                    "❌ Startup validation failed. Unhealthy dependencies: {}",
                    unhealthy_deps.join(", ")
                );
                tracing::error!("{}", error_msg);
                Err(anyhow::anyhow!(error_msg))
            }
            ServiceStatus::Starting => Err(anyhow::anyhow!("⏳ Service is still starting up")),
        }
    }

    /// Check if service is ready to accept traffic (for readiness probes)
    pub async fn is_ready(&self) -> Result<bool> {
        let health_status = self.check_startup_health().await?;

        match health_status.status {
            ServiceStatus::Healthy | ServiceStatus::Degraded => {
                tracing::debug!("Service is ready to accept traffic");
                Ok(true)
            }
            ServiceStatus::Unhealthy | ServiceStatus::Starting => {
                tracing::debug!(
                    "Service is not ready to accept traffic: {:?}",
                    health_status.status
                );
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limiter::RateLimiter;

    #[tokio::test]
    async fn test_health_check_manager_creation() {
        let rate_limiter = Arc::new(RateLimiter::new("redis://127.0.0.1:6379").unwrap());
        let health_manager = HealthCheckManager::new(rate_limiter);

        // Should be able to create health manager
        assert!(health_manager.startup_time.elapsed().as_millis() < 100);
    }

    #[tokio::test]
    async fn test_quick_health_check() {
        let rate_limiter = Arc::new(RateLimiter::new("redis://127.0.0.1:6379").unwrap());
        let health_manager = HealthCheckManager::new(rate_limiter);

        // Quick health check should complete quickly
        let start = Instant::now();
        let _result = health_manager.quick_health_check().await;
        let duration = start.elapsed();

        // Should complete within reasonable time
        assert!(duration < Duration::from_secs(5));
    }

    #[test]
    fn test_service_status_serialization() {
        let status = ServiceStatus::Healthy;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ServiceStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_health_metrics_serialization() {
        let metrics = HealthMetrics {
            uptime_seconds: 3600,
            memory_usage_mb: Some(128),
            cpu_usage_percent: Some(15.5),
            active_connections: Some(42),
        };

        let json = serde_json::to_string(&metrics).unwrap();
        let deserialized: HealthMetrics = serde_json::from_str(&json).unwrap();

        assert_eq!(metrics.uptime_seconds, deserialized.uptime_seconds);
        assert_eq!(metrics.memory_usage_mb, deserialized.memory_usage_mb);
    }
}
