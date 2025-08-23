use super::*;
use crate::audit::{
    audit_event::{ActorInfo, AuditEvent, AuditEventType, AuditOutcome, ResourceInfo},
    audit_filter::{AuditFilter, AuditFilterType},
    audit_logger::AuditLogger,
    audit_storage::{AuditStorage, RedisAuditStorage},
    digital_signer::DigitalSigner,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

#[tokio::test]
async fn test_audit_event_creation() {
    let actor = ActorInfo::new()
        .with_api_key("test-key-123".to_string())
        .with_ip_address("192.168.1.1".to_string());

    let resource = ResourceInfo::new("rate_limiter".to_string())
        .with_id("test-resource".to_string());

    let event = AuditEvent::new(
        AuditEventType::ApiRequest,
        actor,
        resource,
        "check_rate_limit".to_string(),
        AuditOutcome::Success,
    )
    .with_metadata("key".to_string(), serde_json::Value::String("test-key".to_string()))
    .with_tenant_id("tenant-123".to_string());

    assert_eq!(event.event_type, AuditEventType::ApiRequest);
    assert_eq!(event.action, "check_rate_limit");
    assert_eq!(event.outcome, AuditOutcome::Success);
    assert_eq!(event.tenant_id, Some("tenant-123".to_string()));
    assert!(event.metadata.contains_key("key"));
}

#[tokio::test]
async fn test_digital_signer() {
    let key = "this-is-a-test-signing-key-that-is-long-enough-for-security";
    let signer = DigitalSigner::new(key).unwrap();

    let message = "test audit event data";
    let signature = signer.sign(message).unwrap();

    assert!(signer.verify(message, &signature).unwrap());
    assert!(!signer.verify("different message", &signature).unwrap());
}

#[tokio::test]
async fn test_audit_event_signing() {
    let key = "test-signing-key-that-is-long-enough-for-security-requirements";
    let signer = DigitalSigner::new(key).unwrap();

    let actor = ActorInfo::new().with_api_key("test-key".to_string());
    let resource = ResourceInfo::new("test".to_string());
    let event = AuditEvent::new(
        AuditEventType::Authentication,
        actor,
        resource,
        "login".to_string(),
        AuditOutcome::Success,
    );

    let canonical_string = event.canonical_string();
    let signature = signer.sign(&canonical_string).unwrap();
    let signed_event = event.with_signature(signature.clone());

    assert_eq!(signed_event.signature, Some(signature));
    assert!(signer.verify(&canonical_string, &signed_event.signature.unwrap()).unwrap());
}

#[tokio::test]
async fn test_audit_filters() {
    let health_filter = AuditFilter::health_check_filter();
    let system_filter = AuditFilter::system_actor_filter();

    // Test health check filter
    let health_event = AuditEvent::new(
        AuditEventType::SystemEvent,
        ActorInfo::new(),
        ResourceInfo::new("health".to_string()),
        "check".to_string(),
        AuditOutcome::Success,
    );

    assert!(health_filter.should_filter(&health_event));

    // Test system actor filter
    let system_event = AuditEvent::new(
        AuditEventType::SystemEvent,
        ActorInfo::new().with_api_key("system".to_string()),
        ResourceInfo::new("config".to_string()),
        "update".to_string(),
        AuditOutcome::Success,
    );

    assert!(system_filter.should_filter(&system_event));

    // Test normal event should not be filtered
    let normal_event = AuditEvent::new(
        AuditEventType::ApiRequest,
        ActorInfo::new().with_api_key("user-key".to_string()),
        ResourceInfo::new("rate_limiter".to_string()),
        "check".to_string(),
        AuditOutcome::Success,
    );

    assert!(!health_filter.should_filter(&normal_event));
    assert!(!system_filter.should_filter(&normal_event));
}

#[tokio::test]
async fn test_sensitive_data_detection() {
    let mut event = AuditEvent::new(
        AuditEventType::Authentication,
        ActorInfo::new(),
        ResourceInfo::new("auth".to_string()),
        "login".to_string(),
        AuditOutcome::Success,
    );

    // Event without sensitive data
    assert!(!event.contains_sensitive_data());

    // Add sensitive data
    event = event.with_metadata("password".to_string(), serde_json::Value::String("secret123".to_string()));
    assert!(event.contains_sensitive_data());

    // Test redaction
    let redacted = event.redacted();
    assert_eq!(
        redacted.metadata.get("password").unwrap(),
        &serde_json::Value::String("[REDACTED]".to_string())
    );
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::Arc;

    async fn create_test_audit_logger() -> Arc<AuditLogger> {
        // Use in-memory storage for testing
        let storage = Box::new(TestAuditStorage::new());
        let signer = DigitalSigner::new("test-key-for-audit-system-that-is-long-enough").unwrap();
        let filters = vec![AuditFilter::health_check_filter().disabled()]; // Disable for testing

        Arc::new(AuditLogger::new(storage, signer, filters).await.unwrap())
    }

    #[tokio::test]
    async fn test_audit_logger_integration() {
        let audit_logger = create_test_audit_logger().await;

        let actor = ActorInfo::new()
            .with_api_key("test-api-key".to_string())
            .with_ip_address("192.168.1.100".to_string());

        // Test API request logging
        audit_logger
            .log_api_request(
                actor.clone(),
                "POST",
                "/v1/check",
                200,
                Some("tenant-123".to_string()),
                Some(Uuid::new_v4()),
            )
            .await
            .unwrap();

        // Test authentication logging
        let mut metadata = HashMap::new();
        metadata.insert("method".to_string(), serde_json::Value::String("api_key".to_string()));

        audit_logger
            .log_authentication(
                actor.clone(),
                "authenticate",
                AuditOutcome::Success,
                Some("tenant-123".to_string()),
                Some(metadata),
            )
            .await
            .unwrap();

        // Test security event logging
        audit_logger
            .log_security_event(
                actor.clone(),
                "rate_limit_exceeded",
                "rate_limiter",
                AuditOutcome::Success,
                Some("tenant-123".to_string()),
                Some("medium"),
                Some("Rate limit exceeded for API key"),
            )
            .await
            .unwrap();

        // Test admin action logging
        audit_logger
            .log_admin_action(
                actor,
                "delete_user_data",
                "user_data",
                Some("user-456"),
                AuditOutcome::Success,
                Some("tenant-123".to_string()),
                Some(serde_json::json!({"records_deleted": 5})),
            )
            .await
            .unwrap();
    }

    // Simple in-memory storage for testing
    struct TestAuditStorage {
        events: tokio::sync::RwLock<Vec<AuditEvent>>,
    }

    impl TestAuditStorage {
        fn new() -> Self {
            Self {
                events: tokio::sync::RwLock::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl AuditStorage for TestAuditStorage {
        async fn store_event(&self, event: &AuditEvent) -> anyhow::Result<()> {
            let mut events = self.events.write().await;
            events.push(event.clone());
            Ok(())
        }

        async fn get_event(&self, event_id: &Uuid) -> anyhow::Result<Option<AuditEvent>> {
            let events = self.events.read().await;
            Ok(events.iter().find(|e| e.id == *event_id).cloned())
        }

        async fn get_events_by_timerange(
            &self,
            start: DateTime<Utc>,
            end: DateTime<Utc>,
            _tenant_id: Option<&str>,
        ) -> anyhow::Result<Vec<AuditEvent>> {
            let events = self.events.read().await;
            Ok(events
                .iter()
                .filter(|e| e.timestamp >= start && e.timestamp <= end)
                .cloned()
                .collect())
        }

        async fn get_events_by_actor(
            &self,
            actor_id: &str,
            _tenant_id: Option<&str>,
        ) -> anyhow::Result<Vec<AuditEvent>> {
            let events = self.events.read().await;
            Ok(events
                .iter()
                .filter(|e| {
                    e.actor.user_id.as_deref() == Some(actor_id)
                        || e.actor.api_key_id.as_deref() == Some(actor_id)
                })
                .cloned()
                .collect())
        }

        async fn verify_integrity(&self) -> anyhow::Result<bool> {
            Ok(true)
        }
    }
}