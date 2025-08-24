use crate::security::{
    threat_analyzer::{ThreatAnalyzer, ThreatScore, RequestContext, ThreatLevel},
    response_engine::{ResponseEngine, DefensiveAction},
    siem_integration::SiemIntegration,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct ThreatDetector {
    analyzers: Vec<Box<dyn ThreatAnalyzer>>,
    response_engine: Arc<ResponseEngine>,
    siem_integration: Option<Arc<SiemIntegration>>,
    config: Arc<RwLock<ThreatDetectorConfig>>,
}

#[derive(Debug, Clone)]
pub struct ThreatDetectorConfig {
    pub enabled: bool,
    pub threat_threshold: f64,
    pub confidence_threshold: f64,
    pub auto_response_enabled: bool,
    pub analyzer_weights: std::collections::HashMap<String, f64>,
    pub max_analysis_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ThreatAnalysisResult {
    pub correlation_id: Uuid,
    pub overall_score: ThreatScore,
    pub individual_scores: Vec<ThreatScore>,
    pub actions_taken: Vec<DefensiveAction>,
    pub analysis_duration_ms: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ThreatDetector {
    pub fn new(
        analyzers: Vec<Box<dyn ThreatAnalyzer>>,
        response_engine: Arc<ResponseEngine>,
        siem_integration: Option<Arc<SiemIntegration>>,
    ) -> Self {
        let config = ThreatDetectorConfig {
            enabled: true,
            threat_threshold: 0.6,
            confidence_threshold: 0.7,
            auto_response_enabled: true,
            analyzer_weights: std::collections::HashMap::new(),
            max_analysis_time_ms: 5000,
        };

        Self {
            analyzers,
            response_engine,
            siem_integration,
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Analyze a request for threats and optionally take defensive actions
    pub async fn analyze_request(&self, context: &RequestContext) -> Result<ThreatAnalysisResult> {
        let start_time = std::time::Instant::now();
        let config = self.config.read().await;

        if !config.enabled {
            return Ok(ThreatAnalysisResult {
                correlation_id: context.correlation_id,
                overall_score: ThreatScore::new("disabled".to_string(), 0.0, 1.0),
                individual_scores: Vec::new(),
                actions_taken: Vec::new(),
                analysis_duration_ms: 0,
                timestamp: chrono::Utc::now(),
            });
        }

        // Run all analyzers concurrently with timeout
        let analysis_timeout = tokio::time::Duration::from_millis(config.max_analysis_time_ms);
        let mut individual_scores = Vec::new();

        for analyzer in &self.analyzers {
            if !analyzer.is_enabled() {
                continue;
            }

            match tokio::time::timeout(analysis_timeout, analyzer.analyze(context)).await {
                Ok(Ok(score)) => {
                    info!(
                        analyzer = analyzer.analyzer_id(),
                        score = score.score,
                        confidence = score.confidence,
                        "Threat analysis completed"
                    );
                    individual_scores.push(score);
                }
                Ok(Err(e)) => {
                    error!(
                        analyzer = analyzer.analyzer_id(),
                        error = %e,
                        "Threat analyzer failed"
                    );
                }
                Err(_) => {
                    warn!(
                        analyzer = analyzer.analyzer_id(),
                        timeout_ms = config.max_analysis_time_ms,
                        "Threat analyzer timed out"
                    );
                }
            }
        }

        // Combine scores using configured weights
        let weights: Vec<f64> = individual_scores
            .iter()
            .map(|score| {
                config
                    .analyzer_weights
                    .get(&score.analyzer_id)
                    .copied()
                    .unwrap_or(1.0)
            })
            .collect();

        let overall_score = ThreatScore::combine_scores(individual_scores.clone(), Some(weights));

        // Determine if action should be taken
        let mut actions_taken = Vec::new();
        if config.auto_response_enabled
            && overall_score.score >= config.threat_threshold
            && overall_score.confidence >= config.confidence_threshold
        {
            // Take defensive actions
            actions_taken = self
                .response_engine
                .respond_to_threat(context, &overall_score)
                .await?;

            info!(
                correlation_id = %context.correlation_id,
                threat_score = overall_score.score,
                actions_count = actions_taken.len(),
                "Defensive actions taken"
            );
        }

        // Send to SIEM if configured
        if let Some(siem) = &self.siem_integration {
            if let Err(e) = siem
                .send_security_event(context, &overall_score, &actions_taken)
                .await
            {
                error!(error = %e, "Failed to send event to SIEM");
            }
        }

        let analysis_duration = start_time.elapsed().as_millis() as u64;

        Ok(ThreatAnalysisResult {
            correlation_id: context.correlation_id,
            overall_score,
            individual_scores,
            actions_taken,
            analysis_duration_ms: analysis_duration,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Add a new threat analyzer
    pub async fn add_analyzer(&mut self, analyzer: Box<dyn ThreatAnalyzer>) {
        info!(
            analyzer_id = analyzer.analyzer_id(),
            analyzer_name = analyzer.name(),
            "Adding threat analyzer"
        );
        self.analyzers.push(analyzer);
    }

    /// Update detector configuration
    pub async fn update_config(&self, new_config: ThreatDetectorConfig) -> Result<()> {
        let mut config = self.config.write().await;
        *config = new_config;
        info!("Threat detector configuration updated");
        Ok(())
    }

    /// Get current configuration
    pub async fn get_config(&self) -> ThreatDetectorConfig {
        self.config.read().await.clone()
    }

    /// Get statistics about threat detection
    pub async fn get_statistics(&self) -> ThreatDetectorStatistics {
        // This would typically be stored in a metrics system
        // For now, return basic info
        ThreatDetectorStatistics {
            analyzers_count: self.analyzers.len(),
            enabled_analyzers: self
                .analyzers
                .iter()
                .filter(|a| a.is_enabled())
                .count(),
            total_analyses: 0, // Would be tracked in real implementation
            threats_detected: 0,
            actions_taken: 0,
            average_analysis_time_ms: 0.0,
        }
    }

    /// Perform a health check on all analyzers
    pub async fn health_check(&self) -> Result<Vec<AnalyzerHealthStatus>> {
        let mut health_statuses = Vec::new();

        for analyzer in &self.analyzers {
            let status = AnalyzerHealthStatus {
                analyzer_id: analyzer.analyzer_id().to_string(),
                name: analyzer.name().to_string(),
                enabled: analyzer.is_enabled(),
                healthy: true, // Would perform actual health check
                last_error: None,
            };
            health_statuses.push(status);
        }

        Ok(health_statuses)
    }

    /// Enable or disable the threat detector
    pub async fn set_enabled(&self, enabled: bool) {
        let mut config = self.config.write().await;
        config.enabled = enabled;
        info!(enabled = enabled, "Threat detector enabled status changed");
    }

    /// Set the threat threshold for automatic responses
    pub async fn set_threat_threshold(&self, threshold: f64) {
        let mut config = self.config.write().await;
        config.threat_threshold = threshold.clamp(0.0, 1.0);
        info!(threshold = config.threat_threshold, "Threat threshold updated");
    }

    /// Enable or disable automatic responses
    pub async fn set_auto_response(&self, enabled: bool) {
        let mut config = self.config.write().await;
        config.auto_response_enabled = enabled;
        info!(enabled = enabled, "Auto response status changed");
    }
}

#[derive(Debug, Clone)]
pub struct ThreatDetectorStatistics {
    pub analyzers_count: usize,
    pub enabled_analyzers: usize,
    pub total_analyses: u64,
    pub threats_detected: u64,
    pub actions_taken: u64,
    pub average_analysis_time_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerHealthStatus {
    pub analyzer_id: String,
    pub name: String,
    pub enabled: bool,
    pub healthy: bool,
    pub last_error: Option<String>,
}

impl Default for ThreatDetectorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threat_threshold: 0.6,
            confidence_threshold: 0.7,
            auto_response_enabled: true,
            analyzer_weights: std::collections::HashMap::new(),
            max_analysis_time_ms: 5000,
        }
    }
}

impl ThreatAnalysisResult {
    /// Check if this analysis indicates a threat that requires action
    pub fn requires_action(&self) -> bool {
        self.overall_score.level.requires_immediate_action()
    }

    /// Get a summary of the analysis
    pub fn summary(&self) -> String {
        format!(
            "Threat analysis: {} (analyzed by {} systems in {}ms)",
            self.overall_score.summary(),
            self.individual_scores.len(),
            self.analysis_duration_ms
        )
    }

    /// Get the highest individual threat score
    pub fn highest_individual_score(&self) -> Option<&ThreatScore> {
        self.individual_scores
            .iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::threat_analyzer::{ThreatAnalyzer, ThreatScore, RequestContext};
    use async_trait::async_trait;

    struct MockThreatAnalyzer {
        id: String,
        score: f64,
        confidence: f64,
        enabled: bool,
    }

    impl MockThreatAnalyzer {
        fn new(id: String, score: f64, confidence: f64) -> Self {
            Self {
                id,
                score,
                confidence,
                enabled: true,
            }
        }
    }

    #[async_trait]
    impl ThreatAnalyzer for MockThreatAnalyzer {
        async fn analyze(&self, _context: &RequestContext) -> anyhow::Result<ThreatScore> {
            Ok(ThreatScore::new(self.id.clone(), self.score, self.confidence))
        }

        fn analyzer_id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.id
        }

        fn is_enabled(&self) -> bool {
            self.enabled
        }

        async fn update_config(&mut self, _config: serde_json::Value) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_threat_detector_analysis() {
        use crate::security::response_engine::ResponseEngine;

        let analyzers: Vec<Box<dyn ThreatAnalyzer>> = vec![
            Box::new(MockThreatAnalyzer::new("analyzer1".to_string(), 0.3, 0.8)),
            Box::new(MockThreatAnalyzer::new("analyzer2".to_string(), 0.7, 0.9)),
        ];

        let response_engine = Arc::new(ResponseEngine::new(Default::default()));
        let detector = ThreatDetector::new(analyzers, response_engine, None);

        let context = RequestContext::new(
            "192.168.1.1".to_string(),
            "/api/test".to_string(),
            "GET".to_string(),
        );

        let result = detector.analyze_request(&context).await.unwrap();

        assert_eq!(result.individual_scores.len(), 2);
        assert_eq!(result.overall_score.score, 0.5); // Average of 0.3 and 0.7
        assert!(result.analysis_duration_ms > 0);
    }

    #[tokio::test]
    async fn test_threat_detector_disabled() {
        use crate::security::response_engine::ResponseEngine;

        let analyzers: Vec<Box<dyn ThreatAnalyzer>> = vec![
            Box::new(MockThreatAnalyzer::new("analyzer1".to_string(), 0.9, 0.9)),
        ];

        let response_engine = Arc::new(ResponseEngine::new(Default::default()));
        let detector = ThreatDetector::new(analyzers, response_engine, None);

        // Disable the detector
        detector.set_enabled(false).await;

        let context = RequestContext::new(
            "192.168.1.1".to_string(),
            "/api/test".to_string(),
            "GET".to_string(),
        );

        let result = detector.analyze_request(&context).await.unwrap();

        assert_eq!(result.overall_score.score, 0.0);
        assert_eq!(result.individual_scores.len(), 0);
    }
}