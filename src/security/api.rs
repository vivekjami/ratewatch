use crate::security::{ThreatDetector, threat_detector::{ThreatDetectorConfig, ThreatAnalysisResult}};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{error, info};

#[derive(Debug, Deserialize)]
pub struct ThreatConfigUpdate {
    pub enabled: Option<bool>,
    pub threat_threshold: Option<f64>,
    pub confidence_threshold: Option<f64>,
    pub auto_response_enabled: Option<bool>,
    pub max_analysis_time_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ThreatStatsQuery {
    pub include_analyzers: Option<bool>,
    pub include_health: Option<bool>,
}

pub fn create_security_router(threat_detector: Arc<ThreatDetector>) -> Router {
    Router::new()
        .route("/v1/security/threat-detection/status", get(get_threat_detection_status))
        .route("/v1/security/threat-detection/config", get(get_threat_detection_config))
        .route("/v1/security/threat-detection/config", put(update_threat_detection_config))
        .route("/v1/security/threat-detection/statistics", get(get_threat_detection_statistics))
        .route("/v1/security/threat-detection/health", get(get_threat_detection_health))
        .route("/v1/security/threat-detection/enable", post(enable_threat_detection))
        .route("/v1/security/threat-detection/disable", post(disable_threat_detection))
        .with_state(threat_detector)
}

async fn get_threat_detection_status(
    State(threat_detector): State<Arc<ThreatDetector>>,
) -> Result<Json<Value>, StatusCode> {
    let config = threat_detector.get_config().await;
    let statistics = threat_detector.get_statistics().await;

    let response = json!({
        "enabled": config.enabled,
        "auto_response_enabled": config.auto_response_enabled,
        "threat_threshold": config.threat_threshold,
        "confidence_threshold": config.confidence_threshold,
        "analyzers": {
            "total": statistics.analyzers_count,
            "enabled": statistics.enabled_analyzers
        },
        "statistics": {
            "total_analyses": statistics.total_analyses,
            "threats_detected": statistics.threats_detected,
            "actions_taken": statistics.actions_taken,
            "average_analysis_time_ms": statistics.average_analysis_time_ms
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    Ok(Json(response))
}

async fn get_threat_detection_config(
    State(threat_detector): State<Arc<ThreatDetector>>,
) -> Result<Json<Value>, StatusCode> {
    let config = threat_detector.get_config().await;

    let response = json!({
        "enabled": config.enabled,
        "threat_threshold": config.threat_threshold,
        "confidence_threshold": config.confidence_threshold,
        "auto_response_enabled": config.auto_response_enabled,
        "analyzer_weights": config.analyzer_weights,
        "max_analysis_time_ms": config.max_analysis_time_ms,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    Ok(Json(response))
}

async fn update_threat_detection_config(
    State(threat_detector): State<Arc<ThreatDetector>>,
    Json(update): Json<ThreatConfigUpdate>,
) -> Result<Json<Value>, StatusCode> {
    let mut current_config = threat_detector.get_config().await;

    // Apply updates
    if let Some(enabled) = update.enabled {
        current_config.enabled = enabled;
    }
    if let Some(threshold) = update.threat_threshold {
        current_config.threat_threshold = threshold.clamp(0.0, 1.0);
    }
    if let Some(confidence) = update.confidence_threshold {
        current_config.confidence_threshold = confidence.clamp(0.0, 1.0);
    }
    if let Some(auto_response) = update.auto_response_enabled {
        current_config.auto_response_enabled = auto_response;
    }
    if let Some(max_time) = update.max_analysis_time_ms {
        current_config.max_analysis_time_ms = max_time;
    }

    match threat_detector.update_config(current_config.clone()).await {
        Ok(_) => {
            info!("Threat detection configuration updated");
            
            let response = json!({
                "success": true,
                "message": "Threat detection configuration updated successfully",
                "config": {
                    "enabled": current_config.enabled,
                    "threat_threshold": current_config.threat_threshold,
                    "confidence_threshold": current_config.confidence_threshold,
                    "auto_response_enabled": current_config.auto_response_enabled,
                    "max_analysis_time_ms": current_config.max_analysis_time_ms
                },
                "timestamp": chrono::Utc::now().to_rfc3339()
            });

            Ok(Json(response))
        }
        Err(e) => {
            error!(error = %e, "Failed to update threat detection configuration");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_threat_detection_statistics(
    State(threat_detector): State<Arc<ThreatDetector>>,
    Query(query): Query<ThreatStatsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let statistics = threat_detector.get_statistics().await;
    let mut response = json!({
        "statistics": {
            "analyzers_count": statistics.analyzers_count,
            "enabled_analyzers": statistics.enabled_analyzers,
            "total_analyses": statistics.total_analyses,
            "threats_detected": statistics.threats_detected,
            "actions_taken": statistics.actions_taken,
            "average_analysis_time_ms": statistics.average_analysis_time_ms
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    // Include analyzer health if requested
    if query.include_health.unwrap_or(false) {
        match threat_detector.health_check().await {
            Ok(health_statuses) => {
                response["health"] = json!({
                    "analyzers": health_statuses,
                    "overall_healthy": health_statuses.iter().all(|status| status.healthy)
                });
            }
            Err(e) => {
                error!(error = %e, "Failed to get threat detection health status");
                response["health"] = json!({
                    "error": "Failed to retrieve health status"
                });
            }
        }
    }

    Ok(Json(response))
}

async fn get_threat_detection_health(
    State(threat_detector): State<Arc<ThreatDetector>>,
) -> Result<Json<Value>, StatusCode> {
    match threat_detector.health_check().await {
        Ok(health_statuses) => {
            let overall_healthy = health_statuses.iter().all(|status| status.healthy);
            let healthy_count = health_statuses.iter().filter(|status| status.healthy).count();
            
            let response = json!({
                "overall_healthy": overall_healthy,
                "healthy_analyzers": healthy_count,
                "total_analyzers": health_statuses.len(),
                "analyzers": health_statuses,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });

            if overall_healthy {
                Ok(Json(response))
            } else {
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
        Err(e) => {
            error!(error = %e, "Threat detection health check failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn enable_threat_detection(
    State(threat_detector): State<Arc<ThreatDetector>>,
) -> Result<Json<Value>, StatusCode> {
    threat_detector.set_enabled(true).await;
    
    let response = json!({
        "success": true,
        "message": "Threat detection enabled",
        "enabled": true,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    info!("Threat detection enabled via API");
    Ok(Json(response))
}

async fn disable_threat_detection(
    State(threat_detector): State<Arc<ThreatDetector>>,
) -> Result<Json<Value>, StatusCode> {
    threat_detector.set_enabled(false).await;
    
    let response = json!({
        "success": true,
        "message": "Threat detection disabled",
        "enabled": false,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    info!("Threat detection disabled via API");
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{
        response_engine::ResponseEngine,
        threat_analyzer::{ThreatAnalyzer, ThreatScore, RequestContext},
    };
    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::util::ServiceExt;

    struct MockThreatAnalyzer;

    #[async_trait]
    impl ThreatAnalyzer for MockThreatAnalyzer {
        async fn analyze(&self, _context: &RequestContext) -> anyhow::Result<ThreatScore> {
            Ok(ThreatScore::new("mock".to_string(), 0.5, 0.8))
        }

        fn analyzer_id(&self) -> &str {
            "mock"
        }

        fn name(&self) -> &str {
            "Mock Analyzer"
        }

        fn is_enabled(&self) -> bool {
            true
        }

        async fn update_config(&mut self, _config: serde_json::Value) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_threat_detection_status_endpoint() {
        let analyzers: Vec<Box<dyn ThreatAnalyzer>> = vec![Box::new(MockThreatAnalyzer)];
        let response_engine = Arc::new(ResponseEngine::new(Default::default()));
        let threat_detector = Arc::new(ThreatDetector::new(analyzers, response_engine, None));

        let app = create_security_router(threat_detector);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/security/threat-detection/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_enable_disable_threat_detection() {
        let analyzers: Vec<Box<dyn ThreatAnalyzer>> = vec![Box::new(MockThreatAnalyzer)];
        let response_engine = Arc::new(ResponseEngine::new(Default::default()));
        let threat_detector = Arc::new(ThreatDetector::new(analyzers, response_engine, None));

        let app = create_security_router(threat_detector);

        // Test disable
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/security/threat-detection/disable")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Test enable
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/security/threat-detection/enable")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}