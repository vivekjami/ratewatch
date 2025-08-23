pub mod audit_logger;
pub mod audit_storage;
pub mod digital_signer;
pub mod audit_event;
pub mod audit_filter;
pub mod middleware;
pub mod api;

#[cfg(test)]
mod tests;

pub use audit_logger::AuditLogger;
pub use audit_storage::{AuditStorage, RedisAuditStorage, FileAuditStorage};
pub use digital_signer::DigitalSigner;
pub use audit_event::{AuditEvent, AuditEventType, AuditOutcome, ActorInfo, ResourceInfo};
pub use audit_filter::AuditFilter;

use anyhow::Result;
use std::sync::Arc;

/// Initialize the audit system with the specified configuration
pub async fn initialize_audit_system(
    storage_type: &str,
    redis_client: Option<redis::Client>,
    file_path: Option<String>,
    signing_key: &str,
) -> Result<Arc<AuditLogger>> {
    let storage: Box<dyn AuditStorage> = match storage_type {
        "redis" => {
            let client = redis_client.ok_or_else(|| {
                anyhow::anyhow!("Redis client required for Redis audit storage")
            })?;
            Box::new(RedisAuditStorage::new(client))
        }
        "file" => {
            let path = file_path.unwrap_or_else(|| "/var/log/ratewatch/audit.log".to_string());
            Box::new(FileAuditStorage::new(path)?)
        }
        _ => return Err(anyhow::anyhow!("Unsupported audit storage type: {}", storage_type)),
    };

    let signer = DigitalSigner::new(signing_key)?;
    let audit_logger = AuditLogger::new(storage, signer, vec![]).await?;
    
    Ok(Arc::new(audit_logger))
}