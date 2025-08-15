use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use validator::Validate;

pub mod sources;
pub mod secrets;
pub mod validation;

use sources::*;
use secrets::*;
use validation::ConfigValidator;

/// Enterprise configuration manager with multiple sources and hot-reloading
pub struct ConfigManager {
    sources: Vec<Box<dyn ConfigSource>>,
    secret_manager: Arc<SecretManager>,
    validator: ConfigValidator,
    current_config: Arc<RwLock<EnterpriseConfig>>,
    config_id: Uuid,
}

impl ConfigManager {
    pub async fn new() -> Result<Self> {
        let mut sources: Vec<Box<dyn ConfigSource>> = vec![
            Box::new(EnvConfigSource::new()),
            Box::new(FileConfigSource::new("config.toml")?),
        ];

        // Add external sources if configured
        if std::env::var("VAULT_ADDR").is_ok() {
            sources.push(Box::new(VaultConfigSource::new().await?));
        }

        if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
            sources.push(Box::new(K8sConfigSource::new().await?));
        }

        let secret_manager = Arc::new(SecretManager::new().await?);
        let validator = ConfigValidator::new();
        
        // Load initial configuration
        let initial_config = Self::load_merged_config(&sources, &secret_manager).await?;
        validator.validate(&initial_config)?;

        Ok(Self {
            sources,
            secret_manager,
            validator,
            current_config: Arc::new(RwLock::new(initial_config)),
            config_id: Uuid::new_v4(),
        })
    }

    pub async fn get_config(&self) -> EnterpriseConfig {
        self.current_config.read().await.clone()
    }

    pub async fn reload_config(&self) -> Result<()> {
        tracing::info!("Reloading configuration from all sources");
        
        let new_config = Self::load_merged_config(&self.sources, &self.secret_manager).await?;
        self.validator.validate(&new_config)?;

        let mut current = self.current_config.write().await;
        *current = new_config;

        tracing::info!("Configuration reloaded successfully");
        Ok(())
    }

    async fn load_merged_config(
        sources: &[Box<dyn ConfigSource>],
        secret_manager: &SecretManager,
    ) -> Result<EnterpriseConfig> {
        let mut merged_config = ConfigMap::new();

        // Load from all sources in order (later sources override earlier ones)
        for source in sources {
            match source.load_config().await {
                Ok(config) => {
                    merged_config.merge(config);
                    tracing::debug!("Loaded configuration from source: {}", source.name());
                }
                Err(e) => {
                    tracing::warn!("Failed to load from source {}: {}", source.name(), e);
                }
            }
        }

        // Resolve secrets
        Self::resolve_secrets(&mut merged_config, secret_manager).await?;

        // Convert to typed configuration
        let config: EnterpriseConfig = if merged_config.is_empty() {
            EnterpriseConfig::default()
        } else {
            merged_config.try_into()
                .context("Failed to parse configuration")?
        };

        Ok(config)
    }

    async fn resolve_secrets(
        config: &mut ConfigMap,
        secret_manager: &SecretManager,
    ) -> Result<()> {
        for (key, value) in config.iter_mut() {
            if let Some(secret_ref) = value.as_str() {
                if secret_ref.starts_with("secret://") {
                    let secret_key = &secret_ref[9..]; // Remove "secret://" prefix
                    match secret_manager.get_secret(secret_key).await {
                        Ok(secret_value) => {
                            *value = serde_json::Value::String(secret_value);
                            tracing::debug!("Resolved secret for key: {}", key);
                        }
                        Err(e) => {
                            tracing::error!("Failed to resolve secret {}: {}", secret_key, e);
                            return Err(e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn watch_changes(&self) -> Result<tokio::sync::mpsc::Receiver<ConfigChangeEvent>> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        for source in &self.sources {
            let mut change_stream = source.watch_changes().await?;
            let tx_clone = tx.clone();
            let config_manager = self.current_config.clone();
            let validator = ConfigValidator::new();
            let secret_manager = self.secret_manager.clone();
            
            tokio::spawn(async move {
                while let Some(change) = change_stream.recv().await {
                    tracing::info!("Configuration change detected: {:?}", change);
                    
                    // Attempt to reload and validate
                    match Self::load_merged_config(&[], &secret_manager).await {
                        Ok(new_config) => {
                            if let Err(e) = validator.validate(&new_config) {
                                tracing::error!("Configuration validation failed: {}", e);
                                let _ = tx_clone.send(ConfigChangeEvent::ValidationFailed(e)).await;
                                continue;
                            }

                            let mut current = config_manager.write().await;
                            *current = new_config;
                            
                            let _ = tx_clone.send(ConfigChangeEvent::Updated).await;
                        }
                        Err(e) => {
                            tracing::error!("Failed to reload configuration: {}", e);
                            let _ = tx_clone.send(ConfigChangeEvent::LoadFailed(e)).await;
                        }
                    }
                }
            });
        }

        Ok(rx)
    }
}

/// Configuration change events
#[derive(Debug)]
pub enum ConfigChangeEvent {
    Updated,
    ValidationFailed(anyhow::Error),
    LoadFailed(anyhow::Error),
}

/// Configuration source trait
#[async_trait]
pub trait ConfigSource: Send + Sync {
    async fn load_config(&self) -> Result<ConfigMap>;
    async fn watch_changes(&self) -> Result<tokio::sync::mpsc::Receiver<ConfigChange>>;
    fn name(&self) -> &str;
}

/// Configuration change notification
#[derive(Debug, Clone)]
pub struct ConfigChange {
    pub source: String,
    pub change_type: ConfigChangeType,
    pub affected_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum ConfigChangeType {
    Added,
    Modified,
    Removed,
}

/// Type alias for configuration map
pub type ConfigMap = HashMap<String, serde_json::Value>;

/// Enterprise configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EnterpriseConfig {
    #[validate(nested)]
    pub server: ServerConfig,
    #[validate(nested)]
    pub security: SecurityConfig,
    #[validate(nested)]
    pub observability: ObservabilityConfig,
    #[validate(nested)]
    pub tenancy: TenancyConfig,
    #[validate(nested)]
    pub disaster_recovery: DisasterRecoveryConfig,
    #[validate(nested)]
    pub infrastructure: InfrastructureConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ServerConfig {
    #[validate(range(min = 1024, max = 65535))]
    pub port: u16,
    #[validate(length(min = 1))]
    pub host: String,
    #[validate(range(min = 1, max = 1000))]
    pub worker_threads: usize,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TlsConfig {
    #[validate(length(min = 1))]
    pub cert_path: String,
    #[validate(length(min = 1))]
    pub key_path: String,
    pub ca_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SecurityConfig {
    #[validate(nested)]
    pub audit: AuditConfig,
    #[validate(nested)]
    pub threat_detection: ThreatDetectionConfig,
    #[validate(nested)]
    pub secrets: SecretConfig,
    #[validate(nested)]
    pub compliance: ComplianceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AuditConfig {
    pub enabled: bool,
    #[validate(length(min = 1))]
    pub storage_backend: String,
    pub digital_signing: bool,
    #[validate(range(min = 1))]
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ThreatDetectionConfig {
    pub enabled: bool,
    pub behavioral_analysis: bool,
    pub ip_reputation: bool,
    pub ml_engine: bool,
    #[validate(range(min = 0.0, max = 1.0))]
    pub threat_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SecretConfig {
    #[validate(length(min = 1))]
    pub provider: String,
    pub vault_config: Option<VaultConfig>,
    pub aws_config: Option<AwsSecretsConfig>,
    pub azure_config: Option<AzureKeyVaultConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VaultConfig {
    #[validate(url)]
    pub address: String,
    pub token: Option<String>,
    pub role_id: Option<String>,
    pub secret_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AwsSecretsConfig {
    pub region: String,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AzureKeyVaultConfig {
    #[validate(url)]
    pub vault_url: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub tenant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ComplianceConfig {
    pub gdpr_enabled: bool,
    pub ccpa_enabled: bool,
    pub data_residency: Option<String>,
    #[validate(range(min = 1))]
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ObservabilityConfig {
    #[validate(nested)]
    pub metrics: MetricsConfig,
    #[validate(nested)]
    pub tracing: TracingConfig,
    #[validate(nested)]
    pub alerting: AlertingConfig,
    #[validate(nested)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MetricsConfig {
    pub enabled: bool,
    #[validate(length(min = 1))]
    pub endpoint: String,
    pub push_gateway: Option<String>,
    pub collection_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TracingConfig {
    pub enabled: bool,
    #[validate(length(min = 1))]
    pub service_name: String,
    pub jaeger_endpoint: Option<String>,
    pub sampling_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AlertingConfig {
    pub enabled: bool,
    pub channels: Vec<AlertChannel>,
    pub thresholds: AlertThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertChannel {
    pub name: String,
    pub channel_type: String,
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AlertThresholds {
    #[validate(range(min = 0.0))]
    pub error_rate: f64,
    #[validate(range(min = 0.0))]
    pub response_time_p99: f64,
    #[validate(range(min = 0.0))]
    pub cpu_usage: f64,
    #[validate(range(min = 0.0))]
    pub memory_usage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LoggingConfig {
    #[validate(length(min = 1))]
    pub level: String,
    #[validate(length(min = 1))]
    pub format: String,
    pub structured: bool,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TenancyConfig {
    pub enabled: bool,
    #[validate(nested)]
    pub default_quotas: ResourceQuotas,
    pub isolation_level: IsolationLevel,
    pub billing_integration: Option<BillingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResourceQuotas {
    #[validate(range(min = 1))]
    pub max_requests_per_second: u64,
    #[validate(range(min = 1))]
    pub max_api_keys: u32,
    #[validate(range(min = 1))]
    pub max_storage_mb: u64,
    #[validate(range(min = 1))]
    pub max_retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IsolationLevel {
    Strict,
    Moderate,
    Relaxed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BillingConfig {
    #[validate(length(min = 1))]
    pub provider: String,
    #[validate(url)]
    pub webhook_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DisasterRecoveryConfig {
    #[validate(nested)]
    pub backup: BackupConfig,
    #[validate(nested)]
    pub replication: ReplicationConfig,
    #[validate(nested)]
    pub failover: FailoverConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackupConfig {
    pub enabled: bool,
    pub storage_backends: Vec<String>,
    #[validate(length(min = 1))]
    pub schedule: String,
    #[validate(range(min = 1))]
    pub retention_days: u32,
    pub encryption_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReplicationConfig {
    pub enabled: bool,
    pub replicas: Vec<ReplicaConfig>,
    pub sync_mode: SyncMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReplicaConfig {
    #[validate(length(min = 1))]
    pub name: String,
    #[validate(url)]
    pub endpoint: String,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMode {
    Synchronous,
    Asynchronous,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FailoverConfig {
    pub enabled: bool,
    #[validate(range(min = 1))]
    pub health_check_interval_seconds: u64,
    #[validate(range(min = 1))]
    pub failure_threshold: u32,
    pub automatic_failback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct InfrastructureConfig {
    #[validate(nested)]
    pub cloud_providers: Vec<CloudProviderConfig>,
    #[validate(nested)]
    pub auto_scaling: AutoScalingConfig,
    #[validate(nested)]
    pub deployment: DeploymentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CloudProviderConfig {
    #[validate(length(min = 1))]
    pub name: String,
    #[validate(length(min = 1))]
    pub provider_type: String,
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AutoScalingConfig {
    pub enabled: bool,
    #[validate(range(min = 1))]
    pub min_instances: u32,
    #[validate(range(min = 1))]
    pub max_instances: u32,
    pub metrics: Vec<ScalingMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ScalingMetric {
    #[validate(length(min = 1))]
    pub name: String,
    #[validate(range(min = 0.0))]
    pub target_value: f64,
    #[validate(range(min = 1))]
    pub evaluation_periods: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DeploymentConfig {
    #[validate(length(min = 1))]
    pub strategy: String,
    pub blue_green: Option<BlueGreenConfig>,
    pub canary: Option<CanaryConfig>,
    pub rollback: RollbackConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BlueGreenConfig {
    #[validate(range(min = 1))]
    pub health_check_timeout_seconds: u64,
    #[validate(range(min = 1))]
    pub traffic_switch_delay_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CanaryConfig {
    #[validate(range(min = 1, max = 100))]
    pub initial_traffic_percent: u8,
    #[validate(range(min = 1, max = 100))]
    pub traffic_increment_percent: u8,
    #[validate(range(min = 1))]
    pub evaluation_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RollbackConfig {
    pub automatic: bool,
    #[validate(range(min = 1))]
    pub failure_threshold: u32,
    #[validate(range(min = 1))]
    pub timeout_seconds: u64,
}

impl Default for EnterpriseConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                port: 8081,
                host: "0.0.0.0".to_string(),
                worker_threads: 4,
                tls: None,
            },
            security: SecurityConfig {
                audit: AuditConfig {
                    enabled: true,
                    storage_backend: "redis".to_string(),
                    digital_signing: true,
                    retention_days: 90,
                },
                threat_detection: ThreatDetectionConfig {
                    enabled: true,
                    behavioral_analysis: true,
                    ip_reputation: true,
                    ml_engine: false,
                    threat_threshold: 0.7,
                },
                secrets: SecretConfig {
                    provider: "env".to_string(),
                    vault_config: None,
                    aws_config: None,
                    azure_config: None,
                },
                compliance: ComplianceConfig {
                    gdpr_enabled: true,
                    ccpa_enabled: true,
                    data_residency: None,
                    retention_days: 30,
                },
            },
            observability: ObservabilityConfig {
                metrics: MetricsConfig {
                    enabled: true,
                    endpoint: "/metrics".to_string(),
                    push_gateway: None,
                    collection_interval_seconds: 15,
                },
                tracing: TracingConfig {
                    enabled: true,
                    service_name: "ratewatch".to_string(),
                    jaeger_endpoint: None,
                    sampling_rate: 0.1,
                },
                alerting: AlertingConfig {
                    enabled: true,
                    channels: vec![],
                    thresholds: AlertThresholds {
                        error_rate: 0.05,
                        response_time_p99: 500.0,
                        cpu_usage: 80.0,
                        memory_usage: 85.0,
                    },
                },
                logging: LoggingConfig {
                    level: "info".to_string(),
                    format: "json".to_string(),
                    structured: true,
                    file_path: None,
                },
            },
            tenancy: TenancyConfig {
                enabled: false,
                default_quotas: ResourceQuotas {
                    max_requests_per_second: 1000,
                    max_api_keys: 10,
                    max_storage_mb: 100,
                    max_retention_days: 30,
                },
                isolation_level: IsolationLevel::Strict,
                billing_integration: None,
            },
            disaster_recovery: DisasterRecoveryConfig {
                backup: BackupConfig {
                    enabled: true,
                    storage_backends: vec!["local".to_string()],
                    schedule: "0 2 * * *".to_string(), // Daily at 2 AM
                    retention_days: 30,
                    encryption_enabled: true,
                },
                replication: ReplicationConfig {
                    enabled: false,
                    replicas: vec![],
                    sync_mode: SyncMode::Asynchronous,
                },
                failover: FailoverConfig {
                    enabled: false,
                    health_check_interval_seconds: 30,
                    failure_threshold: 3,
                    automatic_failback: false,
                },
            },
            infrastructure: InfrastructureConfig {
                cloud_providers: vec![],
                auto_scaling: AutoScalingConfig {
                    enabled: false,
                    min_instances: 1,
                    max_instances: 10,
                    metrics: vec![],
                },
                deployment: DeploymentConfig {
                    strategy: "rolling".to_string(),
                    blue_green: None,
                    canary: None,
                    rollback: RollbackConfig {
                        automatic: true,
                        failure_threshold: 3,
                        timeout_seconds: 300,
                    },
                },
            },
        }
    }
}

impl TryFrom<ConfigMap> for EnterpriseConfig {
    type Error = anyhow::Error;

    fn try_from(config_map: ConfigMap) -> Result<Self> {
        // Convert flattened config map to nested structure
        let nested_config = Self::unflatten_config(config_map)?;
        
        serde_json::from_value(nested_config)
            .context("Failed to deserialize configuration")
    }
}

impl EnterpriseConfig {
    fn unflatten_config(config_map: ConfigMap) -> Result<serde_json::Value> {
        let mut nested = serde_json::Map::new();
        
        for (key, value) in config_map {
            let parts: Vec<&str> = key.split('.').collect();
            Self::insert_nested(&mut nested, &parts, value);
        }
        
        Ok(serde_json::Value::Object(nested))
    }
    
    fn insert_nested(map: &mut serde_json::Map<String, serde_json::Value>, parts: &[&str], value: serde_json::Value) {
        if parts.is_empty() {
            return;
        }
        
        if parts.len() == 1 {
            map.insert(parts[0].to_string(), value);
            return;
        }
        
        let key = parts[0];
        let remaining = &parts[1..];
        
        let entry = map.entry(key.to_string()).or_insert_with(|| {
            serde_json::Value::Object(serde_json::Map::new())
        });
        
        if let serde_json::Value::Object(ref mut obj) = entry {
            Self::insert_nested(obj, remaining, value);
        }
    }
}

trait ConfigMapExt {
    fn merge(&mut self, other: ConfigMap);
}

impl ConfigMapExt for ConfigMap {
    fn merge(&mut self, other: ConfigMap) {
        for (key, value) in other {
            self.insert(key, value);
        }
    }
}