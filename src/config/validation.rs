use super::EnterpriseConfig;
use anyhow::{Context, Result};
use validator::Validate;

/// Configuration validator
pub struct ConfigValidator {
    // Remove Clone derive since Box<dyn CustomValidator> doesn't implement Clone
}

impl ConfigValidator {
    pub fn new() -> Self {
        Self {
            // Initialize without custom validators for now
        }
    }

    pub fn validate(&self, config: &EnterpriseConfig) -> Result<()> {
        // First run the built-in validator derive validation
        config.validate()
            .context("Configuration validation failed")?;

        // Run built-in custom validators
        SecurityValidator.validate(config)?;
        NetworkValidator.validate(config)?;
        ResourceValidator.validate(config)?;
        ComplianceValidator.validate(config)?;

        tracing::debug!("Configuration validation passed");
        Ok(())
    }
}

/// Custom validator trait
pub trait CustomValidator: Send + Sync {
    fn validate(&self, config: &EnterpriseConfig) -> Result<()>;
    fn name(&self) -> &str;
}

/// Security configuration validator
pub struct SecurityValidator;

impl CustomValidator for SecurityValidator {
    fn validate(&self, config: &EnterpriseConfig) -> Result<()> {
        // Validate API key secret strength
        if let Ok(api_key_secret) = std::env::var("API_KEY_SECRET") {
            if api_key_secret.len() < 32 {
                return Err(anyhow::anyhow!(
                    "API_KEY_SECRET must be at least 32 characters long"
                ));
            }
            
            if api_key_secret == "change-this-in-production" || 
               api_key_secret == "change-this-to-a-secure-random-string-minimum-32-characters" {
                return Err(anyhow::anyhow!(
                    "API_KEY_SECRET must be changed from default value in production"
                ));
            }
        }

        // Validate TLS configuration in production
        if std::env::var("ENVIRONMENT").unwrap_or_default() == "production" {
            if config.server.tls.is_none() {
                tracing::warn!("TLS is not configured in production environment");
            }
        }

        // Validate audit configuration
        if config.security.audit.enabled && config.security.audit.retention_days < 30 {
            tracing::warn!("Audit retention period is less than 30 days, which may not meet compliance requirements");
        }

        // Validate threat detection thresholds
        if config.security.threat_detection.enabled {
            let threshold = config.security.threat_detection.threat_threshold;
            if threshold < 0.1 || threshold > 0.9 {
                return Err(anyhow::anyhow!(
                    "Threat detection threshold must be between 0.1 and 0.9, got: {}", threshold
                ));
            }
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "security"
    }
}

/// Network configuration validator
pub struct NetworkValidator;

impl CustomValidator for NetworkValidator {
    fn validate(&self, config: &EnterpriseConfig) -> Result<()> {
        // Validate port ranges
        if config.server.port < 1024 && std::env::var("ALLOW_PRIVILEGED_PORTS").is_err() {
            return Err(anyhow::anyhow!(
                "Port {} requires privileged access. Set ALLOW_PRIVILEGED_PORTS=true to override",
                config.server.port
            ));
        }

        // Validate host binding
        if config.server.host == "0.0.0.0" {
            if std::env::var("ENVIRONMENT").unwrap_or_default() == "production" {
                tracing::warn!("Binding to 0.0.0.0 in production may expose service to unintended networks");
            }
        }

        // Validate Redis URL format
        if let Ok(redis_url) = std::env::var("REDIS_URL") {
            if !redis_url.starts_with("redis://") && !redis_url.starts_with("rediss://") {
                return Err(anyhow::anyhow!(
                    "REDIS_URL must start with redis:// or rediss://, got: {}", redis_url
                ));
            }
        }

        // Validate external service URLs
        if let Some(jaeger_endpoint) = &config.observability.tracing.jaeger_endpoint {
            if !jaeger_endpoint.starts_with("http://") && !jaeger_endpoint.starts_with("https://") {
                return Err(anyhow::anyhow!(
                    "Jaeger endpoint must be a valid HTTP/HTTPS URL: {}", jaeger_endpoint
                ));
            }
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "network"
    }
}

/// Resource configuration validator
pub struct ResourceValidator;

impl CustomValidator for ResourceValidator {
    fn validate(&self, config: &EnterpriseConfig) -> Result<()> {
        // Validate worker thread count
        let cpu_count = num_cpus::get();
        if config.server.worker_threads > cpu_count * 2 {
            tracing::warn!(
                "Worker thread count ({}) is more than 2x CPU count ({}), this may cause performance issues",
                config.server.worker_threads, cpu_count
            );
        }

        // Validate tenant quotas
        if config.tenancy.enabled {
            let quotas = &config.tenancy.default_quotas;
            
            if quotas.max_requests_per_second > 1_000_000 {
                tracing::warn!("Very high RPS quota may impact system performance: {}", quotas.max_requests_per_second);
            }
            
            if quotas.max_storage_mb > 10_000 {
                tracing::warn!("Very high storage quota may impact system performance: {} MB", quotas.max_storage_mb);
            }
        }

        // Validate auto-scaling configuration
        if config.infrastructure.auto_scaling.enabled {
            let auto_scaling = &config.infrastructure.auto_scaling;
            
            if auto_scaling.min_instances >= auto_scaling.max_instances {
                return Err(anyhow::anyhow!(
                    "Auto-scaling min_instances ({}) must be less than max_instances ({})",
                    auto_scaling.min_instances, auto_scaling.max_instances
                ));
            }
            
            if auto_scaling.max_instances > 100 {
                tracing::warn!("Very high max_instances may lead to unexpected costs: {}", auto_scaling.max_instances);
            }
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "resource"
    }
}

/// Compliance configuration validator
pub struct ComplianceValidator;

impl CustomValidator for ComplianceValidator {
    fn validate(&self, config: &EnterpriseConfig) -> Result<()> {
        let compliance = &config.security.compliance;

        // Validate data retention periods for compliance
        if compliance.gdpr_enabled || compliance.ccpa_enabled {
            if compliance.retention_days > 2555 { // ~7 years
                tracing::warn!(
                    "Data retention period of {} days may exceed regulatory requirements",
                    compliance.retention_days
                );
            }
            
            if compliance.retention_days < 1 {
                return Err(anyhow::anyhow!(
                    "Data retention period must be at least 1 day for compliance tracking"
                ));
            }
        }

        // Validate audit retention for compliance
        if (compliance.gdpr_enabled || compliance.ccpa_enabled) && config.security.audit.enabled {
            if config.security.audit.retention_days < 90 {
                tracing::warn!(
                    "Audit retention period of {} days may not meet compliance requirements (recommended: 90+ days)",
                    config.security.audit.retention_days
                );
            }
        }

        // Validate data residency requirements
        if let Some(data_residency) = &compliance.data_residency {
            let valid_regions = ["us", "eu", "apac", "ca", "uk"];
            if !valid_regions.contains(&data_residency.as_str()) {
                return Err(anyhow::anyhow!(
                    "Invalid data residency region: {}. Valid options: {:?}",
                    data_residency, valid_regions
                ));
            }
        }

        // Validate backup encryption for compliance
        if (compliance.gdpr_enabled || compliance.ccpa_enabled) && 
           config.disaster_recovery.backup.enabled &&
           !config.disaster_recovery.backup.encryption_enabled {
            return Err(anyhow::anyhow!(
                "Backup encryption must be enabled when GDPR or CCPA compliance is required"
            ));
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "compliance"
    }
}

/// Environment-specific validator
pub struct EnvironmentValidator {
    environment: String,
}

impl EnvironmentValidator {
    pub fn new(environment: String) -> Self {
        Self { environment }
    }
}

impl CustomValidator for EnvironmentValidator {
    fn validate(&self, config: &EnterpriseConfig) -> Result<()> {
        match self.environment.as_str() {
            "production" => self.validate_production(config),
            "staging" => self.validate_staging(config),
            "development" => self.validate_development(config),
            _ => Ok(()), // Unknown environment, skip validation
        }
    }

    fn name(&self) -> &str {
        "environment"
    }
}

impl EnvironmentValidator {
    fn validate_production(&self, config: &EnterpriseConfig) -> Result<()> {
        // Production-specific validations
        if config.observability.logging.level == "debug" || config.observability.logging.level == "trace" {
            tracing::warn!("Debug/trace logging enabled in production may impact performance");
        }

        if !config.security.audit.enabled {
            return Err(anyhow::anyhow!("Audit logging must be enabled in production"));
        }

        if !config.disaster_recovery.backup.enabled {
            return Err(anyhow::anyhow!("Backup must be enabled in production"));
        }

        if config.server.tls.is_none() {
            return Err(anyhow::anyhow!("TLS must be configured in production"));
        }

        Ok(())
    }

    fn validate_staging(&self, config: &EnterpriseConfig) -> Result<()> {
        // Staging-specific validations
        if !config.security.audit.enabled {
            tracing::warn!("Audit logging should be enabled in staging to match production");
        }

        Ok(())
    }

    fn validate_development(&self, config: &EnterpriseConfig) -> Result<()> {
        // Development-specific validations (more lenient)
        if config.security.threat_detection.enabled {
            tracing::info!("Threat detection enabled in development environment");
        }

        Ok(())
    }
}

// Helper function to get CPU count (mock implementation)
mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    }
}