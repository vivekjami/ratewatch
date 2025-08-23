use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub actor: ActorInfo,
    pub resource: ResourceInfo,
    pub action: String,
    pub outcome: AuditOutcome,
    pub metadata: HashMap<String, serde_json::Value>,
    pub signature: Option<String>,
    pub correlation_id: Option<Uuid>,
    pub tenant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditEventType {
    Authentication,
    Authorization,
    DataAccess,
    DataModification,
    SystemConfiguration,
    SecurityEvent,
    PrivacyEvent,
    AdminAction,
    ApiRequest,
    SystemEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuditOutcome {
    Success,
    Failure,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<String>,
    pub tenant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub resource_path: Option<String>,
    pub tenant_id: Option<String>,
}

impl AuditEvent {
    pub fn new(
        event_type: AuditEventType,
        actor: ActorInfo,
        resource: ResourceInfo,
        action: String,
        outcome: AuditOutcome,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            actor,
            resource,
            action,
            outcome,
            metadata: HashMap::new(),
            signature: None,
            correlation_id: None,
            tenant_id: None,
        }
    }

    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    pub fn with_signature(mut self, signature: String) -> Self {
        self.signature = Some(signature);
        self
    }

    /// Get the canonical string representation for signing
    pub fn canonical_string(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.id,
            self.timestamp.to_rfc3339(),
            serde_json::to_string(&self.event_type).unwrap_or_default(),
            self.action,
            serde_json::to_string(&self.outcome).unwrap_or_default(),
            serde_json::to_string(&self.actor).unwrap_or_default(),
            serde_json::to_string(&self.resource).unwrap_or_default()
        )
    }

    /// Check if this event contains sensitive information that should be redacted
    pub fn contains_sensitive_data(&self) -> bool {
        // Check for common sensitive data patterns
        let sensitive_keys = ["password", "secret", "token", "key", "credential"];
        
        for (key, value) in &self.metadata {
            if sensitive_keys.iter().any(|&s| key.to_lowercase().contains(s)) {
                return true;
            }
            
            if let Some(str_value) = value.as_str() {
                if sensitive_keys.iter().any(|&s| str_value.to_lowercase().contains(s)) {
                    return true;
                }
            }
        }
        
        false
    }

    /// Create a redacted version of this event for logging
    pub fn redacted(&self) -> Self {
        let mut redacted = self.clone();
        
        // Redact sensitive metadata
        let sensitive_keys = ["password", "secret", "token", "key", "credential"];
        for (key, value) in &mut redacted.metadata {
            if sensitive_keys.iter().any(|&s| key.to_lowercase().contains(s)) {
                *value = serde_json::Value::String("[REDACTED]".to_string());
            }
        }
        
        redacted
    }
}

impl ActorInfo {
    pub fn new() -> Self {
        Self {
            user_id: None,
            api_key_id: None,
            ip_address: None,
            user_agent: None,
            session_id: None,
            tenant_id: None,
        }
    }

    pub fn with_api_key(mut self, api_key_id: String) -> Self {
        self.api_key_id = Some(api_key_id);
        self
    }

    pub fn with_ip_address(mut self, ip_address: String) -> Self {
        self.ip_address = Some(ip_address);
        self
    }

    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }

    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }
}

impl ResourceInfo {
    pub fn new(resource_type: String) -> Self {
        Self {
            resource_type,
            resource_id: None,
            resource_path: None,
            tenant_id: None,
        }
    }

    pub fn with_id(mut self, resource_id: String) -> Self {
        self.resource_id = Some(resource_id);
        self
    }

    pub fn with_path(mut self, resource_path: String) -> Self {
        self.resource_path = Some(resource_path);
        self
    }

    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }
}