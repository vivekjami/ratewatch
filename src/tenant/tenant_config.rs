use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: TenantStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub settings: TenantSettings,
    pub quotas: ResourceQuotas,
    pub features: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TenantStatus {
    Active,
    Suspended,
    Deactivated,
    Provisioning,
    Migrating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSettings {
    pub timezone: String,
    pub locale: String,
    pub data_retention_days: u32,
    pub encryption_enabled: bool,
    pub audit_level: AuditLevel,
    pub rate_limits: RateLimitConfig,
    pub security_settings: SecuritySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditLevel {
    Minimal,
    Standard,
    Comprehensive,
    Forensic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub concurrent_connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySettings {
    pub require_mfa: bool,
    pub session_timeout_minutes: u32,
    pub ip_whitelist: Vec<String>,
    pub allowed_origins: Vec<String>,
    pub password_policy: PasswordPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordPolicy {
    pub min_length: u8,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_numbers: bool,
    pub require_symbols: bool,
    pub max_age_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuotas {
    pub max_api_calls_per_hour: u64,
    pub max_storage_mb: u64,
    pub max_concurrent_requests: u32,
    pub max_users: u32,
    pub max_data_export_mb: u64,
}

impl Default for TenantSettings {
    fn default() -> Self {
        Self {
            timezone: "UTC".to_string(),
            locale: "en-US".to_string(),
            data_retention_days: 365,
            encryption_enabled: true,
            audit_level: AuditLevel::Standard,
            rate_limits: RateLimitConfig {
                requests_per_minute: 1000,
                burst_size: 100,
                concurrent_connections: 50,
            },
            security_settings: SecuritySettings {
                require_mfa: false,
                session_timeout_minutes: 480,
                ip_whitelist: vec![],
                allowed_origins: vec!["*".to_string()],
                password_policy: PasswordPolicy {
                    min_length: 8,
                    require_uppercase: true,
                    require_lowercase: true,
                    require_numbers: true,
                    require_symbols: false,
                    max_age_days: Some(90),
                },
            },
        }
    }
}

impl Default for ResourceQuotas {
    fn default() -> Self {
        Self {
            max_api_calls_per_hour: 10000,
            max_storage_mb: 1024,
            max_concurrent_requests: 100,
            max_users: 50,
            max_data_export_mb: 100,
        }
    }
}

impl TenantConfig {
    pub fn new(name: String, slug: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            slug,
            status: TenantStatus::Provisioning,
            created_at: now,
            updated_at: now,
            settings: TenantSettings::default(),
            quotas: ResourceQuotas::default(),
            features: vec![],
            metadata: HashMap::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == TenantStatus::Active
    }

    pub fn can_access_feature(&self, feature: &str) -> bool {
        self.features.contains(&feature.to_string())
    }

    pub fn update_settings(&mut self, settings: TenantSettings) {
        self.settings = settings;
        self.updated_at = Utc::now();
    }

    pub fn update_quotas(&mut self, quotas: ResourceQuotas) {
        self.quotas = quotas;
        self.updated_at = Utc::now();
    }

    pub fn activate(&mut self) {
        self.status = TenantStatus::Active;
        self.updated_at = Utc::now();
    }

    pub fn suspend(&mut self) {
        self.status = TenantStatus::Suspended;
        self.updated_at = Utc::now();
    }

    pub fn deactivate(&mut self) {
        self.status = TenantStatus::Deactivated;
        self.updated_at = Utc::now();
    }
}