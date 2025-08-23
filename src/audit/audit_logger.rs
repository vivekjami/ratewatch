use crate::audit::{
    audit_event::{AuditEvent, AuditEventType, AuditOutcome, ActorInfo, ResourceInfo},
    audit_filter::{AuditFilter, AuditFilterSet},
    audit_storage::AuditStorage,
    digital_signer::DigitalSigner,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct AuditLogger {
    storage: Box<dyn AuditStorage>,
    signer: DigitalSigner,
    filters: Arc<RwLock<AuditFilterSet>>,
    audit_access_logger: Option<Arc<AuditLogger>>, // For audit-the-auditor functionality
}

impl AuditLogger {
    pub async fn new(
        storage: Box<dyn AuditStorage>,
        signer: DigitalSigner,
        filters: Vec<AuditFilter>,
    ) -> Result<Self> {
        let filter_set = AuditFilterSet::with_filters(filters);
        
        Ok(Self {
            storage,
            signer,
            filters: Arc::new(RwLock::new(filter_set)),
            audit_access_logger: None,
        })
    }

    /// Create an audit logger with audit-the-auditor functionality
    pub async fn with_audit_access_logging(
        storage: Box<dyn AuditStorage>,
        signer: DigitalSigner,
        filters: Vec<AuditFilter>,
        audit_access_storage: Box<dyn AuditStorage>,
        audit_access_signer: DigitalSigner,
    ) -> Result<Self> {
        let audit_access_logger = Arc::new(
            Self::new(audit_access_storage, audit_access_signer, vec![]).await?
        );

        let filter_set = AuditFilterSet::with_filters(filters);
        
        Ok(Self {
            storage,
            signer,
            filters: Arc::new(RwLock::new(filter_set)),
            audit_access_logger: Some(audit_access_logger),
        })
    }

    /// Log an audit event
    pub async fn log_event(&self, mut event: AuditEvent) -> Result<()> {
        // Check if the event should be filtered
        let filters = self.filters.read().await;
        if filters.should_filter(&event) {
            return Ok(()); // Event filtered, don't log
        }
        drop(filters);

        // Sign the event
        let canonical_string = event.canonical_string();
        let signature = self.signer.sign(&canonical_string)?;
        event = event.with_signature(signature);

        // Store the event
        match self.storage.store_event(&event).await {
            Ok(_) => {
                info!(
                    event_id = %event.id,
                    event_type = ?event.event_type,
                    actor = ?event.actor.user_id.as_deref().unwrap_or("unknown"),
                    "Audit event logged successfully"
                );
            }
            Err(e) => {
                error!(
                    event_id = %event.id,
                    error = %e,
                    "Failed to store audit event"
                );
                return Err(e);
            }
        }

        Ok(())
    }

    /// Log an API request event
    pub async fn log_api_request(
        &self,
        actor: ActorInfo,
        method: &str,
        path: &str,
        status_code: u16,
        tenant_id: Option<String>,
        correlation_id: Option<Uuid>,
    ) -> Result<()> {
        let outcome = if status_code < 400 {
            AuditOutcome::Success
        } else if status_code < 500 {
            AuditOutcome::Failure
        } else {
            AuditOutcome::Unknown
        };

        let resource = ResourceInfo::new("api_endpoint".to_string())
            .with_path(path.to_string());

        let mut event = AuditEvent::new(
            AuditEventType::ApiRequest,
            actor,
            resource,
            method.to_string(),
            outcome,
        );

        if let Some(tid) = tenant_id {
            event = event.with_tenant_id(tid);
        }

        if let Some(cid) = correlation_id {
            event = event.with_correlation_id(cid);
        }

        event = event.with_metadata(
            "status_code".to_string(),
            serde_json::Value::Number(status_code.into()),
        );

        self.log_event(event).await
    }

    /// Log an authentication event
    pub async fn log_authentication(
        &self,
        actor: ActorInfo,
        action: &str,
        outcome: AuditOutcome,
        tenant_id: Option<String>,
        metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
    ) -> Result<()> {
        let resource = ResourceInfo::new("authentication".to_string());

        let mut event = AuditEvent::new(
            AuditEventType::Authentication,
            actor,
            resource,
            action.to_string(),
            outcome,
        );

        if let Some(tid) = tenant_id {
            event = event.with_tenant_id(tid);
        }

        if let Some(meta) = metadata {
            for (key, value) in meta {
                event = event.with_metadata(key, value);
            }
        }

        self.log_event(event).await
    }

    /// Log a security event
    pub async fn log_security_event(
        &self,
        actor: ActorInfo,
        action: &str,
        resource_type: &str,
        outcome: AuditOutcome,
        tenant_id: Option<String>,
        threat_level: Option<&str>,
        details: Option<&str>,
    ) -> Result<()> {
        let resource = ResourceInfo::new(resource_type.to_string());

        let mut event = AuditEvent::new(
            AuditEventType::SecurityEvent,
            actor,
            resource,
            action.to_string(),
            outcome,
        );

        if let Some(tid) = tenant_id {
            event = event.with_tenant_id(tid);
        }

        if let Some(level) = threat_level {
            event = event.with_metadata(
                "threat_level".to_string(),
                serde_json::Value::String(level.to_string()),
            );
        }

        if let Some(details) = details {
            event = event.with_metadata(
                "details".to_string(),
                serde_json::Value::String(details.to_string()),
            );
        }

        self.log_event(event).await
    }

    /// Log an administrative action
    pub async fn log_admin_action(
        &self,
        actor: ActorInfo,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        outcome: AuditOutcome,
        tenant_id: Option<String>,
        changes: Option<serde_json::Value>,
    ) -> Result<()> {
        let mut resource = ResourceInfo::new(resource_type.to_string());
        
        if let Some(id) = resource_id {
            resource = resource.with_id(id.to_string());
        }

        let mut event = AuditEvent::new(
            AuditEventType::AdminAction,
            actor,
            resource,
            action.to_string(),
            outcome,
        );

        if let Some(tid) = tenant_id {
            event = event.with_tenant_id(tid);
        }

        if let Some(changes) = changes {
            event = event.with_metadata("changes".to_string(), changes);
        }

        self.log_event(event).await
    }

    /// Retrieve audit events (with audit-the-auditor logging)
    pub async fn get_events_by_timerange(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        tenant_id: Option<&str>,
        accessor: ActorInfo,
    ) -> Result<Vec<AuditEvent>> {
        // Log the audit access
        if let Some(audit_access_logger) = &self.audit_access_logger {
            let resource = ResourceInfo::new("audit_log".to_string());
            let event = AuditEvent::new(
                AuditEventType::DataAccess,
                accessor.clone(),
                resource,
                "query_by_timerange".to_string(),
                AuditOutcome::Success,
            )
            .with_metadata("start_time".to_string(), serde_json::Value::String(start.to_rfc3339()))
            .with_metadata("end_time".to_string(), serde_json::Value::String(end.to_rfc3339()));

            if let Err(e) = audit_access_logger.log_event(event).await {
                warn!("Failed to log audit access: {}", e);
            }
        }

        self.storage.get_events_by_timerange(start, end, tenant_id).await
    }

    /// Retrieve audit events by actor (with audit-the-auditor logging)
    pub async fn get_events_by_actor(
        &self,
        actor_id: &str,
        tenant_id: Option<&str>,
        accessor: ActorInfo,
    ) -> Result<Vec<AuditEvent>> {
        // Log the audit access
        if let Some(audit_access_logger) = &self.audit_access_logger {
            let resource = ResourceInfo::new("audit_log".to_string());
            let event = AuditEvent::new(
                AuditEventType::DataAccess,
                accessor.clone(),
                resource,
                "query_by_actor".to_string(),
                AuditOutcome::Success,
            )
            .with_metadata("target_actor".to_string(), serde_json::Value::String(actor_id.to_string()));

            if let Err(e) = audit_access_logger.log_event(event).await {
                warn!("Failed to log audit access: {}", e);
            }
        }

        self.storage.get_events_by_actor(actor_id, tenant_id).await
    }

    /// Verify the integrity of an audit event
    pub async fn verify_event_integrity(&self, event: &AuditEvent) -> Result<bool> {
        if let Some(signature) = &event.signature {
            let canonical_string = event.canonical_string();
            self.signer.verify(&canonical_string, signature)
        } else {
            Ok(false) // No signature means no integrity verification possible
        }
    }

    /// Verify the integrity of the entire audit storage
    pub async fn verify_storage_integrity(&self) -> Result<bool> {
        self.storage.verify_integrity().await
    }

    /// Add a new audit filter
    pub async fn add_filter(&self, filter: AuditFilter) {
        let mut filters = self.filters.write().await;
        *filters = filters.clone().add_filter(filter);
    }

    /// Enable or disable a filter
    pub async fn set_filter_enabled(&self, filter_name: &str, enabled: bool) {
        let mut filters = self.filters.write().await;
        filters.set_filter_enabled(filter_name, enabled);
    }

    /// Get statistics about audit events
    pub async fn get_audit_statistics(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        tenant_id: Option<&str>,
    ) -> Result<AuditStatistics> {
        let events = self.storage.get_events_by_timerange(start, end, tenant_id).await?;
        
        let mut stats = AuditStatistics::new();
        
        for event in &events {
            stats.total_events += 1;
            
            match event.event_type {
                AuditEventType::Authentication => stats.authentication_events += 1,
                AuditEventType::Authorization => stats.authorization_events += 1,
                AuditEventType::DataAccess => stats.data_access_events += 1,
                AuditEventType::DataModification => stats.data_modification_events += 1,
                AuditEventType::SecurityEvent => stats.security_events += 1,
                AuditEventType::ApiRequest => stats.api_request_events += 1,
                _ => stats.other_events += 1,
            }
            
            match event.outcome {
                AuditOutcome::Success => stats.successful_events += 1,
                AuditOutcome::Failure => stats.failed_events += 1,
                _ => stats.other_outcome_events += 1,
            }
        }
        
        Ok(stats)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditStatistics {
    pub total_events: u64,
    pub authentication_events: u64,
    pub authorization_events: u64,
    pub data_access_events: u64,
    pub data_modification_events: u64,
    pub security_events: u64,
    pub api_request_events: u64,
    pub other_events: u64,
    pub successful_events: u64,
    pub failed_events: u64,
    pub other_outcome_events: u64,
}

impl AuditStatistics {
    pub fn new() -> Self {
        Self {
            total_events: 0,
            authentication_events: 0,
            authorization_events: 0,
            data_access_events: 0,
            data_modification_events: 0,
            security_events: 0,
            api_request_events: 0,
            other_events: 0,
            successful_events: 0,
            failed_events: 0,
            other_outcome_events: 0,
        }
    }
}

impl Default for AuditStatistics {
    fn default() -> Self {
        Self::new()
    }
}