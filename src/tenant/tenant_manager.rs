use super::{TenantConfig, TenantStatus, ResourceQuotas, TenantSettings};
use super::resource_quota::{QuotaManager, ResourceType, QuotaViolation};
use super::isolation::{TenantIsolationManager, TenantContext, IsolationLevel, DataClassification};
use uuid::Uuid;
use std::collections::HashMap;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantOnboardingRequest {
    pub name: String,
    pub slug: String,
    pub admin_email: String,
    pub organization: String,
    pub isolation_level: IsolationLevel,
    pub data_classification: DataClassification,
    pub initial_quotas: Option<ResourceQuotas>,
    pub initial_settings: Option<TenantSettings>,
    pub features: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantProvisioningStatus {
    pub tenant_id: Uuid,
    pub status: ProvisioningStep,
    pub progress_percentage: u8,
    pub current_step: String,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProvisioningStep {
    Initializing,
    CreatingNamespace,
    SettingUpIsolation,
    ConfiguringQuotas,
    InitializingStorage,
    RunningHealthChecks,
    Completed,
    Failed,
}

pub struct TenantManager {
    redis_client: redis::Client,
    pub quota_manager: QuotaManager,
    isolation_manager: TenantIsolationManager,
    tenant_cache: HashMap<Uuid, TenantConfig>,
}

impl TenantManager {
    pub fn new(redis_url: &str, namespace_prefix: String) -> Result<Self> {
        let redis_client = redis::Client::open(redis_url)?;
        let quota_manager = QuotaManager::new(redis_url)?;
        let isolation_manager = TenantIsolationManager::new(redis_url, namespace_prefix)?;

        Ok(Self {
            redis_client,
            quota_manager,
            isolation_manager,
            tenant_cache: HashMap::new(),
        })
    }

    pub async fn create_tenant(&mut self, request: TenantOnboardingRequest) -> Result<Uuid> {
        // Validate slug uniqueness
        if self.tenant_exists_by_slug(&request.slug).await? {
            return Err(anyhow!("Tenant with slug '{}' already exists", request.slug));
        }

        let mut tenant_config = TenantConfig::new(request.name, request.slug);
        
        // Apply custom settings if provided
        if let Some(quotas) = request.initial_quotas {
            tenant_config.quotas = quotas;
        }
        
        if let Some(settings) = request.initial_settings {
            tenant_config.settings = settings;
        }

        tenant_config.features = request.features;
        tenant_config.metadata = request.metadata;
        tenant_config.metadata.insert("admin_email".to_string(), request.admin_email);
        tenant_config.metadata.insert("organization".to_string(), request.organization);

        // Start provisioning process
        let provisioning_status = TenantProvisioningStatus {
            tenant_id: tenant_config.id,
            status: ProvisioningStep::Initializing,
            progress_percentage: 0,
            current_step: "Starting tenant provisioning".to_string(),
            error_message: None,
            started_at: Utc::now(),
            completed_at: None,
        };

        self.save_provisioning_status(&provisioning_status).await?;
        
        // Save initial tenant config
        self.save_tenant_config(&tenant_config).await?;

        // Start async provisioning
        let tenant_id = tenant_config.id;
        tokio::spawn(async move {
            // This would be handled by the provisioning process
            // For now, we'll simulate it
        });

        Ok(tenant_id)
    }

    pub async fn provision_tenant(&mut self, tenant_id: Uuid, request: TenantOnboardingRequest) -> Result<()> {
        let mut status = self.get_provisioning_status(tenant_id).await?
            .ok_or_else(|| anyhow!("Provisioning status not found"))?;

        // Step 1: Create namespace
        status.status = ProvisioningStep::CreatingNamespace;
        status.progress_percentage = 20;
        status.current_step = "Creating tenant namespace".to_string();
        self.save_provisioning_status(&status).await?;

        let context = self.isolation_manager.create_tenant_context(
            tenant_id,
            request.isolation_level,
            request.data_classification,
        );

        // Step 2: Set up isolation
        status.status = ProvisioningStep::SettingUpIsolation;
        status.progress_percentage = 40;
        status.current_step = "Setting up data isolation".to_string();
        self.save_provisioning_status(&status).await?;

        // Initialize tenant namespace with metadata
        self.isolation_manager.set_tenant_data(
            &context,
            "metadata",
            &serde_json::to_string(&request.metadata)?,
            None,
        ).await?;

        // Step 3: Configure quotas
        status.status = ProvisioningStep::ConfiguringQuotas;
        status.progress_percentage = 60;
        status.current_step = "Configuring resource quotas".to_string();
        self.save_provisioning_status(&status).await?;

        // Initialize usage tracking
        self.quota_manager.update_usage(tenant_id, ResourceType::ApiCalls, 0).await?;

        // Step 4: Initialize storage
        status.status = ProvisioningStep::InitializingStorage;
        status.progress_percentage = 80;
        status.current_step = "Initializing tenant storage".to_string();
        self.save_provisioning_status(&status).await?;

        // Create initial tenant data structures
        let tenant_config = self.get_tenant_config(tenant_id).await?;
        let config_json = serde_json::to_string(&tenant_config)?;
        self.isolation_manager.set_tenant_data(
            &context,
            "config",
            &config_json,
            None,
        ).await?;

        // Step 5: Health checks
        status.status = ProvisioningStep::RunningHealthChecks;
        status.progress_percentage = 90;
        status.current_step = "Running health checks".to_string();
        self.save_provisioning_status(&status).await?;

        // Verify tenant can be accessed
        let health_check = self.health_check_tenant(tenant_id).await?;
        if !health_check {
            status.status = ProvisioningStep::Failed;
            status.error_message = Some("Health check failed".to_string());
            self.save_provisioning_status(&status).await?;
            return Err(anyhow!("Tenant provisioning failed health check"));
        }

        // Step 6: Complete
        status.status = ProvisioningStep::Completed;
        status.progress_percentage = 100;
        status.current_step = "Provisioning completed".to_string();
        status.completed_at = Some(Utc::now());
        self.save_provisioning_status(&status).await?;

        // Activate tenant
        let mut tenant_config = self.get_tenant_config(tenant_id).await?;
        tenant_config.activate();
        self.save_tenant_config(&tenant_config).await?;

        Ok(())
    }

    pub async fn get_tenant_config(&mut self, tenant_id: Uuid) -> Result<TenantConfig> {
        // Check cache first
        if let Some(config) = self.tenant_cache.get(&tenant_id) {
            return Ok(config.clone());
        }

        // Fetch from Redis
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("tenant:{}:config", tenant_id);
        
        let config_data: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        let config = config_data
            .ok_or_else(|| anyhow!("Tenant not found"))?;
        
        let tenant_config: TenantConfig = serde_json::from_str(&config)?;
        self.tenant_cache.insert(tenant_id, tenant_config.clone());
        
        Ok(tenant_config)
    }

    pub async fn update_tenant_config(&mut self, tenant_id: Uuid, config: TenantConfig) -> Result<()> {
        self.save_tenant_config(&config).await?;
        self.tenant_cache.insert(tenant_id, config);
        Ok(())
    }

    pub async fn get_tenant_by_slug(&mut self, slug: &str) -> Result<Option<TenantConfig>> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("tenant:slug:{}", slug);
        
        let tenant_id: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        if let Some(id_str) = tenant_id {
            let tenant_id = Uuid::parse_str(&id_str)?;
            Ok(Some(self.get_tenant_config(tenant_id).await?))
        } else {
            Ok(None)
        }
    }

    pub async fn list_tenants(&mut self, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<TenantConfig>> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let pattern = "tenant:*:config";
        
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(pattern)
            .query_async(&mut conn)
            .await?;

        let mut tenants = Vec::new();
        let start = offset.unwrap_or(0) as usize;
        let end = if let Some(limit) = limit {
            std::cmp::min(start + limit as usize, keys.len())
        } else {
            keys.len()
        };

        for key in &keys[start..end] {
            let config_data: Option<String> = redis::cmd("GET")
                .arg(key)
                .query_async(&mut conn)
                .await?;

            if let Some(data) = config_data {
                let tenant_config: TenantConfig = serde_json::from_str(&data)?;
                tenants.push(tenant_config);
            }
        }

        Ok(tenants)
    }

    pub async fn suspend_tenant(&mut self, tenant_id: Uuid, reason: String) -> Result<()> {
        let mut config = self.get_tenant_config(tenant_id).await?;
        config.suspend();
        config.metadata.insert("suspension_reason".to_string(), reason);
        config.metadata.insert("suspended_at".to_string(), Utc::now().to_rfc3339());
        
        self.save_tenant_config(&config).await?;
        Ok(())
    }

    pub async fn reactivate_tenant(&mut self, tenant_id: Uuid) -> Result<()> {
        let mut config = self.get_tenant_config(tenant_id).await?;
        config.activate();
        config.metadata.remove("suspension_reason");
        config.metadata.remove("suspended_at");
        
        self.save_tenant_config(&config).await?;
        Ok(())
    }

    pub async fn delete_tenant(&mut self, tenant_id: Uuid) -> Result<()> {
        let config = self.get_tenant_config(tenant_id).await?;
        
        // Create isolation context for data purge
        let context = self.isolation_manager.create_tenant_context(
            tenant_id,
            IsolationLevel::Shared, // Default for deletion
            DataClassification::Internal,
        );

        // Purge all tenant data
        self.isolation_manager.purge_tenant_data(&context).await?;

        // Remove tenant config
        let mut conn = self.redis_client.get_async_connection().await?;
        let config_key = format!("tenant:{}:config", tenant_id);
        let slug_key = format!("tenant:slug:{}", config.slug);
        
        redis::cmd("DEL")
            .arg(&config_key)
            .arg(&slug_key)
            .query_async(&mut conn)
            .await?;

        // Remove from cache
        self.tenant_cache.remove(&tenant_id);
        
        Ok(())
    }

    pub async fn check_quota_violations(&mut self, tenant_id: Uuid) -> Result<Vec<QuotaViolation>> {
        let config = self.get_tenant_config(tenant_id).await?;
        self.quota_manager.check_quota_violation(tenant_id, &config.quotas).await
    }

    pub async fn health_check_tenant(&mut self, tenant_id: Uuid) -> Result<bool> {
        // Check if tenant config exists and is accessible
        let config = self.get_tenant_config(tenant_id).await?;
        
        // Check if tenant namespace is accessible
        let context = self.isolation_manager.create_tenant_context(
            tenant_id,
            IsolationLevel::Shared,
            DataClassification::Internal,
        );

        // Try to access tenant data
        let test_result = self.isolation_manager.get_tenant_data(&context, "health_check").await?;
        
        // Set health check timestamp
        self.isolation_manager.set_tenant_data(
            &context,
            "health_check",
            &Utc::now().to_rfc3339(),
            Some(300), // 5 minutes TTL
        ).await?;

        Ok(config.is_active())
    }

    async fn save_tenant_config(&self, config: &TenantConfig) -> Result<()> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let config_key = format!("tenant:{}:config", config.id);
        let slug_key = format!("tenant:slug:{}", config.slug);
        let config_json = serde_json::to_string(config)?;
        
        // Save config
        redis::cmd("SET")
            .arg(&config_key)
            .arg(&config_json)
            .query_async(&mut conn)
            .await?;

        // Save slug mapping
        redis::cmd("SET")
            .arg(&slug_key)
            .arg(config.id.to_string())
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    async fn tenant_exists_by_slug(&self, slug: &str) -> Result<bool> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("tenant:slug:{}", slug);
        
        let exists: bool = redis::cmd("EXISTS")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        Ok(exists)
    }

    async fn save_provisioning_status(&self, status: &TenantProvisioningStatus) -> Result<()> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("tenant:{}:provisioning", status.tenant_id);
        let status_json = serde_json::to_string(status)?;
        
        redis::cmd("SET")
            .arg(&key)
            .arg(&status_json)
            .arg("EX")
            .arg(86400) // 24 hours TTL
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    async fn get_provisioning_status(&self, tenant_id: Uuid) -> Result<Option<TenantProvisioningStatus>> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("tenant:{}:provisioning", tenant_id);
        
        let status_data: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        if let Some(data) = status_data {
            let status: TenantProvisioningStatus = serde_json::from_str(&data)?;
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }
}