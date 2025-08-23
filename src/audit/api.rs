use crate::audit::{AuditLogger, audit_event::ActorInfo};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub actor_id: Option<String>,
    pub tenant_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct AuditQueryResponse {
    pub events: Vec<serde_json::Value>,
    pub total_count: usize,
    pub query_info: AuditQueryInfo,
}

#[derive(Debug, Serialize)]
pub struct AuditQueryInfo {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub tenant_id: Option<String>,
    pub actor_id: Option<String>,
    pub query_timestamp: DateTime<Utc>,
}

pub fn create_audit_router(audit_logger: Arc<AuditLogger>) -> Router {
    Router::new()
        .route("/v1/audit/events", get(query_audit_events))
        .route("/v1/audit/statistics", get(get_audit_statistics))
        .route("/v1/audit/health", get(audit_system_health))
        .with_state(audit_logger)
}

async fn query_audit_events(
    State(audit_logger): State<Arc<AuditLogger>>,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<AuditQueryResponse>, StatusCode> {
    // Default time range: last 24 hours
    let end_time = params.end_time.unwrap_or_else(Utc::now);
    let start_time = params
        .start_time
        .unwrap_or_else(|| end_time - chrono::Duration::hours(24));

    // Create accessor info for audit-the-auditor
    let accessor = ActorInfo::new(); // Would be populated from request context

    let events = if let Some(actor_id) = &params.actor_id {
        // Query by actor
        audit_logger
            .get_events_by_actor(actor_id, params.tenant_id.as_deref(), accessor)
            .await
            .map_err(|e| {
                tracing::error!("Failed to query audit events by actor: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    } else {
        // Query by time range
        audit_logger
            .get_events_by_timerange(start_time, end_time, params.tenant_id.as_deref(), accessor)
            .await
            .map_err(|e| {
                tracing::error!("Failed to query audit events by timerange: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    };

    // Apply limit if specified
    let limited_events = if let Some(limit) = params.limit {
        events.into_iter().take(limit).collect()
    } else {
        events
    };

    let total_count = limited_events.len();

    // Convert events to JSON, redacting sensitive data
    let event_json: Vec<Value> = limited_events
        .into_iter()
        .map(|event| {
            let redacted_event = event.redacted();
            serde_json::to_value(redacted_event).unwrap_or_else(|_| json!({}))
        })
        .collect();

    let response = AuditQueryResponse {
        events: event_json,
        total_count,
        query_info: AuditQueryInfo {
            start_time,
            end_time,
            tenant_id: params.tenant_id,
            actor_id: params.actor_id,
            query_timestamp: Utc::now(),
        },
    };

    Ok(Json(response))
}

async fn get_audit_statistics(
    State(audit_logger): State<Arc<AuditLogger>>,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<Value>, StatusCode> {
    // Default time range: last 24 hours
    let end_time = params.end_time.unwrap_or_else(Utc::now);
    let start_time = params
        .start_time
        .unwrap_or_else(|| end_time - chrono::Duration::hours(24));

    let statistics = audit_logger
        .get_audit_statistics(start_time, end_time, params.tenant_id.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get audit statistics: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let response = json!({
        "statistics": statistics,
        "time_range": {
            "start": start_time.to_rfc3339(),
            "end": end_time.to_rfc3339()
        },
        "tenant_id": params.tenant_id,
        "generated_at": Utc::now().to_rfc3339()
    });

    Ok(Json(response))
}

async fn audit_system_health(
    State(audit_logger): State<Arc<AuditLogger>>,
) -> Result<Json<Value>, StatusCode> {
    let integrity_check = audit_logger
        .verify_storage_integrity()
        .await
        .map_err(|e| {
            tracing::error!("Audit system integrity check failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let status = if integrity_check { "healthy" } else { "unhealthy" };

    let response = json!({
        "status": status,
        "integrity_verified": integrity_check,
        "timestamp": Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    });

    match integrity_check {
        true => Ok(Json(response)),
        false => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{
        audit_event::{AuditEvent, AuditEventType, AuditOutcome, ResourceInfo},
        audit_storage::AuditStorage,
        digital_signer::DigitalSigner,
        AuditLogger,
    };
    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use std::sync::Arc;
    use tower::util::ServiceExt;
    use uuid::Uuid;

    // Test storage implementation
    #[derive(Clone)]
    struct TestAuditStorage {
        events: Arc<tokio::sync::RwLock<Vec<AuditEvent>>>,
    }

    impl TestAuditStorage {
        fn new() -> Self {
            Self {
                events: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            }
        }

        async fn add_test_event(&self, event: AuditEvent) {
            let mut events = self.events.write().await;
            events.push(event);
        }
    }

    #[async_trait]
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

    async fn create_test_audit_logger() -> (Arc<AuditLogger>, Arc<TestAuditStorage>) {
        let storage = Arc::new(TestAuditStorage::new());
        let storage_clone = storage.clone();
        let signer = DigitalSigner::new("test-key-for-audit-system-that-is-long-enough").unwrap();

        let audit_logger = Arc::new(
            AuditLogger::new(Box::new(storage.as_ref().clone()), signer, vec![])
                .await
                .unwrap(),
        );

        (audit_logger, storage_clone)
    }

    #[tokio::test]
    async fn test_audit_events_endpoint() {
        let (audit_logger, test_storage) = create_test_audit_logger().await;

        // Add a test event
        let test_event = AuditEvent::new(
            AuditEventType::ApiRequest,
            ActorInfo::new().with_api_key("test-key".to_string()),
            ResourceInfo::new("rate_limiter".to_string()),
            "check".to_string(),
            AuditOutcome::Success,
        );

        test_storage.add_test_event(test_event).await;

        let app = create_audit_router(audit_logger);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_audit_statistics_endpoint() {
        let (audit_logger, _) = create_test_audit_logger().await;
        let app = create_audit_router(audit_logger);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/statistics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_audit_health_endpoint() {
        let (audit_logger, _) = create_test_audit_logger().await;
        let app = create_audit_router(audit_logger);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/audit/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}