use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub tenant_id: Uuid,
    pub api_calls_current_hour: u64,
    pub storage_used_mb: u64,
    pub concurrent_requests: u32,
    pub active_users: u32,
    pub data_exported_mb: u64,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaViolation {
    pub tenant_id: Uuid,
    pub resource_type: ResourceType,
    pub current_usage: u64,
    pub quota_limit: u64,
    pub violation_time: DateTime<Utc>,
    pub severity: ViolationSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceType {
    ApiCalls,
    Storage,
    ConcurrentRequests,
    Users,
    DataExport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ViolationSeverity {
    Warning,   // 80-95% of quota
    Critical,  // 95-100% of quota
    Exceeded,  // Over 100% of quota
}

pub struct QuotaManager {
    redis_client: redis::Client,
    usage_cache: HashMap<Uuid, ResourceUsage>,
}

impl QuotaManager {
    pub fn new(redis_url: &str) -> Result<Self> {
        let redis_client = redis::Client::open(redis_url)?;
        Ok(Self {
            redis_client,
            usage_cache: HashMap::new(),
        })
    }

    pub async fn get_usage(&mut self, tenant_id: Uuid) -> Result<ResourceUsage> {
        // Try cache first
        if let Some(usage) = self.usage_cache.get(&tenant_id) {
            if usage.last_updated > Utc::now() - chrono::Duration::minutes(5) {
                return Ok(usage.clone());
            }
        }

        // Fetch from Redis
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("tenant:{}:usage", tenant_id);
        
        let usage_data: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        let usage = if let Some(data) = usage_data {
            serde_json::from_str(&data)?
        } else {
            ResourceUsage {
                tenant_id,
                api_calls_current_hour: 0,
                storage_used_mb: 0,
                concurrent_requests: 0,
                active_users: 0,
                data_exported_mb: 0,
                last_updated: Utc::now(),
            }
        };

        self.usage_cache.insert(tenant_id, usage.clone());
        Ok(usage)
    }

    pub async fn update_usage(&mut self, tenant_id: Uuid, resource_type: ResourceType, delta: i64) -> Result<()> {
        let mut usage = self.get_usage(tenant_id).await?;
        
        match resource_type {
            ResourceType::ApiCalls => {
                usage.api_calls_current_hour = (usage.api_calls_current_hour as i64 + delta).max(0) as u64;
            }
            ResourceType::Storage => {
                usage.storage_used_mb = (usage.storage_used_mb as i64 + delta).max(0) as u64;
            }
            ResourceType::ConcurrentRequests => {
                usage.concurrent_requests = (usage.concurrent_requests as i64 + delta).max(0) as u32;
            }
            ResourceType::Users => {
                usage.active_users = (usage.active_users as i64 + delta).max(0) as u32;
            }
            ResourceType::DataExport => {
                usage.data_exported_mb = (usage.data_exported_mb as i64 + delta).max(0) as u64;
            }
        }

        usage.last_updated = Utc::now();
        
        // Update Redis
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("tenant:{}:usage", tenant_id);
        let usage_json = serde_json::to_string(&usage)?;
        
        redis::cmd("SET")
            .arg(&key)
            .arg(&usage_json)
            .arg("EX")
            .arg(3600) // Expire after 1 hour
            .query_async(&mut conn)
            .await?;

        self.usage_cache.insert(tenant_id, usage);
        Ok(())
    }

    pub async fn check_quota_violation(
        &mut self, 
        tenant_id: Uuid, 
        quotas: &crate::tenant::ResourceQuotas
    ) -> Result<Vec<QuotaViolation>> {
        let usage = self.get_usage(tenant_id).await?;
        let mut violations = Vec::new();

        // Check API calls
        if let Some(violation) = self.check_resource_quota(
            tenant_id,
            ResourceType::ApiCalls,
            usage.api_calls_current_hour,
            quotas.max_api_calls_per_hour,
        ) {
            violations.push(violation);
        }

        // Check storage
        if let Some(violation) = self.check_resource_quota(
            tenant_id,
            ResourceType::Storage,
            usage.storage_used_mb,
            quotas.max_storage_mb,
        ) {
            violations.push(violation);
        }

        // Check concurrent requests
        if let Some(violation) = self.check_resource_quota(
            tenant_id,
            ResourceType::ConcurrentRequests,
            usage.concurrent_requests as u64,
            quotas.max_concurrent_requests as u64,
        ) {
            violations.push(violation);
        }

        // Check users
        if let Some(violation) = self.check_resource_quota(
            tenant_id,
            ResourceType::Users,
            usage.active_users as u64,
            quotas.max_users as u64,
        ) {
            violations.push(violation);
        }

        // Check data export
        if let Some(violation) = self.check_resource_quota(
            tenant_id,
            ResourceType::DataExport,
            usage.data_exported_mb,
            quotas.max_data_export_mb,
        ) {
            violations.push(violation);
        }

        Ok(violations)
    }

    fn check_resource_quota(
        &self,
        tenant_id: Uuid,
        resource_type: ResourceType,
        current_usage: u64,
        quota_limit: u64,
    ) -> Option<QuotaViolation> {
        let usage_percentage = (current_usage as f64 / quota_limit as f64) * 100.0;

        let severity = if usage_percentage > 100.0 {
            ViolationSeverity::Exceeded
        } else if usage_percentage >= 95.0 {
            ViolationSeverity::Critical
        } else if usage_percentage >= 80.0 {
            ViolationSeverity::Warning
        } else {
            return None;
        };

        Some(QuotaViolation {
            tenant_id,
            resource_type,
            current_usage,
            quota_limit,
            violation_time: Utc::now(),
            severity,
        })
    }

    pub async fn can_consume_resource(
        &mut self,
        tenant_id: Uuid,
        resource_type: ResourceType,
        amount: u64,
        quotas: &crate::tenant::ResourceQuotas,
    ) -> Result<bool> {
        let usage = self.get_usage(tenant_id).await?;

        let would_exceed = match resource_type {
            ResourceType::ApiCalls => {
                usage.api_calls_current_hour + amount > quotas.max_api_calls_per_hour
            }
            ResourceType::Storage => {
                usage.storage_used_mb + amount > quotas.max_storage_mb
            }
            ResourceType::ConcurrentRequests => {
                usage.concurrent_requests + amount as u32 > quotas.max_concurrent_requests
            }
            ResourceType::Users => {
                usage.active_users + amount as u32 > quotas.max_users
            }
            ResourceType::DataExport => {
                usage.data_exported_mb + amount > quotas.max_data_export_mb
            }
        };

        Ok(!would_exceed)
    }

    pub async fn reset_hourly_counters(&mut self) -> Result<()> {
        let mut conn = self.redis_client.get_async_connection().await?;
        
        // Get all tenant usage keys
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg("tenant:*:usage")
            .query_async(&mut conn)
            .await?;

        for key in keys {
            let usage_data: Option<String> = redis::cmd("GET")
                .arg(&key)
                .query_async(&mut conn)
                .await?;

            if let Some(data) = usage_data {
                let mut usage: ResourceUsage = serde_json::from_str(&data)?;
                usage.api_calls_current_hour = 0;
                usage.data_exported_mb = 0;
                usage.last_updated = Utc::now();

                let usage_json = serde_json::to_string(&usage)?;
                redis::cmd("SET")
                    .arg(&key)
                    .arg(&usage_json)
                    .arg("EX")
                    .arg(3600)
                    .query_async(&mut conn)
                    .await?;
            }
        }

        // Clear cache
        self.usage_cache.clear();
        Ok(())
    }
}