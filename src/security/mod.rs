pub mod threat_detector;
pub mod threat_analyzer;
pub mod response_engine;
pub mod ip_reputation;
pub mod behavioral_analyzer;
pub mod siem_integration;
pub mod middleware;
pub mod api;

pub use threat_detector::ThreatDetector;
pub use threat_analyzer::{ThreatAnalyzer, ThreatScore, ThreatLevel};
pub use response_engine::{ResponseEngine, DefensiveAction, ResponseConfig};
pub use ip_reputation::{IpReputationAnalyzer, IpReputationProvider};
pub use behavioral_analyzer::{BehaviorAnalyzer, BehaviorPattern, BehaviorMetrics};
pub use siem_integration::{SiemIntegration, SiemProvider, SecurityEvent};

use anyhow::Result;
use std::sync::Arc;

/// Initialize the security system with threat detection and response capabilities
pub async fn initialize_security_system(
    redis_client: redis::Client,
    config: &crate::config::SecurityConfig,
) -> Result<Arc<ThreatDetector>> {
    // Initialize IP reputation analyzer
    let ip_reputation = Arc::new(IpReputationAnalyzer::new().await?);
    
    // Initialize behavioral analyzer
    let behavior_analyzer = Arc::new(BehaviorAnalyzer::new(redis_client.clone()).await?);
    
    // Initialize response engine
    let response_engine = Arc::new(ResponseEngine::new(
        ResponseConfig::from_security_config(config)
    ));
    
    // Initialize SIEM integration if configured
    let siem_integration = if config.siem.enabled {
        Some(Arc::new(SiemIntegration::new(&config.siem).await?))
    } else {
        None
    };
    
    // Create threat detector with all analyzers
    let threat_detector = ThreatDetector::new(
        vec![
            Box::new(ip_reputation),
            Box::new(behavior_analyzer),
        ],
        response_engine,
        siem_integration,
    );
    
    Ok(Arc::new(threat_detector))
}