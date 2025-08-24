use uuid::Uuid;
use std::collections::HashMap;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub namespace: String,
    pub isolation_level: IsolationLevel,
    pub data_classification: DataClassification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IsolationLevel {
    Shared,      // Shared infrastructure, logical separation
    Dedicated,   // Dedicated resources within shared infrastructure
    Private,     // Completely isolated infrastructure
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

pub struct TenantIsolationManager {
    redis_client: redis::Client,
    namespace_prefix: String,
}

impl TenantIsolationManager {
    pub fn new(redis_url: &str, namespace_prefix: String) -> Result<Self> {
        let redis_client = redis::Client::open(redis_url)?;
        Ok(Self {
            redis_client,
            namespace_prefix,
        })
    }

    pub fn create_tenant_context(
        &self,
        tenant_id: Uuid,
        isolation_level: IsolationLevel,
        data_classification: DataClassification,
    ) -> TenantContext {
        let namespace = format!("{}:tenant:{}", self.namespace_prefix, tenant_id);
        
        TenantContext {
            tenant_id,
            namespace,
            isolation_level,
            data_classification,
        }
    }

    pub fn get_namespaced_key(&self, context: &TenantContext, key: &str) -> String {
        format!("{}:{}", context.namespace, key)
    }

    pub async fn set_tenant_data(
        &self,
        context: &TenantContext,
        key: &str,
        value: &str,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let namespaced_key = self.get_namespaced_key(context, key);

        if let Some(ttl) = ttl_seconds {
            redis::cmd("SET")
                .arg(&namespaced_key)
                .arg(value)
                .arg("EX")
                .arg(ttl)
                .query_async(&mut conn)
                .await?;
        } else {
            redis::cmd("SET")
                .arg(&namespaced_key)
                .arg(value)
                .query_async(&mut conn)
                .await?;
        }

        // Add audit trail for data access
        self.log_data_access(context, "SET", key).await?;
        Ok(())
    }

    pub async fn get_tenant_data(&self, context: &TenantContext, key: &str) -> Result<Option<String>> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let namespaced_key = self.get_namespaced_key(context, key);

        let result: Option<String> = redis::cmd("GET")
            .arg(&namespaced_key)
            .query_async(&mut conn)
            .await?;

        // Add audit trail for data access
        self.log_data_access(context, "GET", key).await?;
        Ok(result)
    }

    pub async fn delete_tenant_data(&self, context: &TenantContext, key: &str) -> Result<bool> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let namespaced_key = self.get_namespaced_key(context, key);

        let deleted: u32 = redis::cmd("DEL")
            .arg(&namespaced_key)
            .query_async(&mut conn)
            .await?;

        // Add audit trail for data access
        self.log_data_access(context, "DELETE", key).await?;
        Ok(deleted > 0)
    }

    pub async fn list_tenant_keys(&self, context: &TenantContext, pattern: &str) -> Result<Vec<String>> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let search_pattern = format!("{}:{}", context.namespace, pattern);

        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&search_pattern)
            .query_async(&mut conn)
            .await?;

        // Remove namespace prefix from keys
        let clean_keys: Vec<String> = keys
            .into_iter()
            .map(|key| {
                key.strip_prefix(&format!("{}:", context.namespace))
                    .unwrap_or(&key)
                    .to_string()
            })
            .collect();

        // Add audit trail for data access
        self.log_data_access(context, "LIST", pattern).await?;
        Ok(clean_keys)
    }

    pub async fn purge_tenant_data(&self, context: &TenantContext) -> Result<u64> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let pattern = format!("{}:*", context.namespace);

        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await?;

        if keys.is_empty() {
            return Ok(0);
        }

        let deleted: u64 = redis::cmd("DEL")
            .arg(&keys)
            .query_async(&mut conn)
            .await?;

        // Add audit trail for data purge
        self.log_data_access(context, "PURGE", &format!("{} keys", keys.len())).await?;
        Ok(deleted)
    }

    pub async fn validate_cross_tenant_access(
        &self,
        requesting_context: &TenantContext,
        target_tenant_id: Uuid,
    ) -> Result<bool> {
        // Only allow cross-tenant access for specific isolation levels and data classifications
        match requesting_context.isolation_level {
            IsolationLevel::Private => {
                // Private tenants cannot access other tenant data
                Ok(requesting_context.tenant_id == target_tenant_id)
            }
            IsolationLevel::Dedicated => {
                // Dedicated tenants can only access their own data unless explicitly allowed
                if requesting_context.tenant_id == target_tenant_id {
                    Ok(true)
                } else {
                    self.check_cross_tenant_permission(requesting_context, target_tenant_id).await
                }
            }
            IsolationLevel::Shared => {
                // Shared tenants can access other shared tenant data with proper permissions
                self.check_cross_tenant_permission(requesting_context, target_tenant_id).await
            }
        }
    }

    async fn check_cross_tenant_permission(
        &self,
        requesting_context: &TenantContext,
        target_tenant_id: Uuid,
    ) -> Result<bool> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let permission_key = format!(
            "{}:permissions:{}:{}",
            self.namespace_prefix,
            requesting_context.tenant_id,
            target_tenant_id
        );

        let has_permission: Option<String> = redis::cmd("GET")
            .arg(&permission_key)
            .query_async(&mut conn)
            .await?;

        Ok(has_permission.is_some())
    }

    pub async fn grant_cross_tenant_access(
        &self,
        granting_tenant_id: Uuid,
        requesting_tenant_id: Uuid,
        permissions: Vec<String>,
        ttl_seconds: Option<u64>,
    ) -> Result<()> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let permission_key = format!(
            "{}:permissions:{}:{}",
            self.namespace_prefix,
            requesting_tenant_id,
            granting_tenant_id
        );

        let permissions_json = serde_json::to_string(&permissions)?;

        if let Some(ttl) = ttl_seconds {
            redis::cmd("SET")
                .arg(&permission_key)
                .arg(&permissions_json)
                .arg("EX")
                .arg(ttl)
                .query_async(&mut conn)
                .await?;
        } else {
            redis::cmd("SET")
                .arg(&permission_key)
                .arg(&permissions_json)
                .query_async(&mut conn)
                .await?;
        }

        Ok(())
    }

    pub async fn revoke_cross_tenant_access(
        &self,
        granting_tenant_id: Uuid,
        requesting_tenant_id: Uuid,
    ) -> Result<()> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let permission_key = format!(
            "{}:permissions:{}:{}",
            self.namespace_prefix,
            requesting_tenant_id,
            granting_tenant_id
        );

        redis::cmd("DEL")
            .arg(&permission_key)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    async fn log_data_access(&self, context: &TenantContext, operation: &str, key: &str) -> Result<()> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let audit_key = format!("{}:audit:data_access", context.namespace);
        
        let audit_entry = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "tenant_id": context.tenant_id,
            "operation": operation,
            "key": key,
            "isolation_level": context.isolation_level,
            "data_classification": context.data_classification,
        });

        redis::cmd("LPUSH")
            .arg(&audit_key)
            .arg(audit_entry.to_string())
            .query_async(&mut conn)
            .await?;

        // Keep only last 1000 audit entries per tenant
        redis::cmd("LTRIM")
            .arg(&audit_key)
            .arg(0)
            .arg(999)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    pub async fn get_tenant_metrics(&self, context: &TenantContext) -> Result<TenantMetrics> {
        let mut conn = self.redis_client.get_async_connection().await?;
        
        // Count keys in tenant namespace
        let pattern = format!("{}:*", context.namespace);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await?;

        let key_count = keys.len() as u64;
        
        // Calculate approximate memory usage
        let mut total_memory = 0u64;
        for key in &keys {
            let memory: u64 = redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(key)
                .query_async(&mut conn)
                .await
                .unwrap_or(0);
            total_memory += memory;
        }

        // Get audit log count
        let audit_key = format!("{}:audit:data_access", context.namespace);
        let audit_count: u64 = redis::cmd("LLEN")
            .arg(&audit_key)
            .query_async(&mut conn)
            .await
            .unwrap_or(0);

        Ok(TenantMetrics {
            tenant_id: context.tenant_id,
            key_count,
            memory_usage_bytes: total_memory,
            audit_log_entries: audit_count,
            isolation_level: context.isolation_level.clone(),
            data_classification: context.data_classification.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TenantMetrics {
    pub tenant_id: Uuid,
    pub key_count: u64,
    pub memory_usage_bytes: u64,
    pub audit_log_entries: u64,
    pub isolation_level: IsolationLevel,
    pub data_classification: DataClassification,
}