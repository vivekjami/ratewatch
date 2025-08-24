use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatScore {
    pub score: f64,           // 0.0 to 1.0, where 1.0 is highest threat
    pub level: ThreatLevel,
    pub confidence: f64,      // 0.0 to 1.0, confidence in the assessment
    pub reasons: Vec<String>, // Human-readable reasons for the score
    pub metadata: HashMap<String, serde_json::Value>,
    pub analyzer_id: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreatLevel {
    None,      // 0.0 - 0.1
    Low,       // 0.1 - 0.3
    Medium,    // 0.3 - 0.6
    High,      // 0.6 - 0.8
    Critical,  // 0.8 - 1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    pub correlation_id: Uuid,
    pub ip_address: String,
    pub user_agent: Option<String>,
    pub api_key_id: Option<String>,
    pub tenant_id: Option<String>,
    pub endpoint: String,
    pub method: String,
    pub timestamp: DateTime<Utc>,
    pub headers: HashMap<String, String>,
    pub rate_limit_key: Option<String>,
    pub previous_requests: Vec<PreviousRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviousRequest {
    pub timestamp: DateTime<Utc>,
    pub endpoint: String,
    pub status_code: u16,
    pub response_time_ms: u64,
}

#[async_trait]
pub trait ThreatAnalyzer: Send + Sync {
    /// Analyze a request and return a threat score
    async fn analyze(&self, context: &RequestContext) -> anyhow::Result<ThreatScore>;
    
    /// Get the analyzer's unique identifier
    fn analyzer_id(&self) -> &str;
    
    /// Get the analyzer's display name
    fn name(&self) -> &str;
    
    /// Check if the analyzer is enabled
    fn is_enabled(&self) -> bool;
    
    /// Update analyzer configuration
    async fn update_config(&mut self, config: serde_json::Value) -> anyhow::Result<()>;
}

impl ThreatScore {
    pub fn new(analyzer_id: String, score: f64, confidence: f64) -> Self {
        let level = ThreatLevel::from_score(score);
        
        Self {
            score,
            level,
            confidence,
            reasons: Vec::new(),
            metadata: HashMap::new(),
            analyzer_id,
            timestamp: Utc::now(),
        }
    }
    
    pub fn with_reason(mut self, reason: String) -> Self {
        self.reasons.push(reason);
        self
    }
    
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    pub fn with_reasons(mut self, reasons: Vec<String>) -> Self {
        self.reasons = reasons;
        self
    }
    
    /// Combine multiple threat scores using weighted average
    pub fn combine_scores(scores: Vec<ThreatScore>, weights: Option<Vec<f64>>) -> ThreatScore {
        if scores.is_empty() {
            return ThreatScore::new("combined".to_string(), 0.0, 0.0);
        }
        
        let weights = weights.unwrap_or_else(|| vec![1.0; scores.len()]);
        let total_weight: f64 = weights.iter().sum();
        
        if total_weight == 0.0 {
            return ThreatScore::new("combined".to_string(), 0.0, 0.0);
        }
        
        let weighted_score: f64 = scores
            .iter()
            .zip(weights.iter())
            .map(|(score, weight)| score.score * weight)
            .sum::<f64>() / total_weight;
            
        let weighted_confidence: f64 = scores
            .iter()
            .zip(weights.iter())
            .map(|(score, weight)| score.confidence * weight)
            .sum::<f64>() / total_weight;
        
        let mut combined_reasons = Vec::new();
        let mut combined_metadata = HashMap::new();
        
        for score in &scores {
            combined_reasons.extend(score.reasons.clone());
            for (key, value) in &score.metadata {
                combined_metadata.insert(
                    format!("{}_{}", score.analyzer_id, key),
                    value.clone(),
                );
            }
        }
        
        ThreatScore::new("combined".to_string(), weighted_score, weighted_confidence)
            .with_reasons(combined_reasons)
    }
    
    /// Check if this threat score indicates an actionable threat
    pub fn is_actionable(&self, threshold: f64) -> bool {
        self.score >= threshold && self.confidence >= 0.5
    }
    
    /// Get a human-readable summary of the threat
    pub fn summary(&self) -> String {
        let level_str = match self.level {
            ThreatLevel::None => "No threat",
            ThreatLevel::Low => "Low threat",
            ThreatLevel::Medium => "Medium threat",
            ThreatLevel::High => "High threat",
            ThreatLevel::Critical => "Critical threat",
        };
        
        format!(
            "{} (score: {:.2}, confidence: {:.2})",
            level_str, self.score, self.confidence
        )
    }
}

impl ThreatLevel {
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s < 0.1 => ThreatLevel::None,
            s if s < 0.3 => ThreatLevel::Low,
            s if s < 0.6 => ThreatLevel::Medium,
            s if s < 0.8 => ThreatLevel::High,
            _ => ThreatLevel::Critical,
        }
    }
    
    pub fn to_score_range(&self) -> (f64, f64) {
        match self {
            ThreatLevel::None => (0.0, 0.1),
            ThreatLevel::Low => (0.1, 0.3),
            ThreatLevel::Medium => (0.3, 0.6),
            ThreatLevel::High => (0.6, 0.8),
            ThreatLevel::Critical => (0.8, 1.0),
        }
    }
    
    pub fn requires_immediate_action(&self) -> bool {
        matches!(self, ThreatLevel::High | ThreatLevel::Critical)
    }
}

impl RequestContext {
    pub fn new(
        ip_address: String,
        endpoint: String,
        method: String,
    ) -> Self {
        Self {
            correlation_id: Uuid::new_v4(),
            ip_address,
            user_agent: None,
            api_key_id: None,
            tenant_id: None,
            endpoint,
            method,
            timestamp: Utc::now(),
            headers: HashMap::new(),
            rate_limit_key: None,
            previous_requests: Vec::new(),
        }
    }
    
    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }
    
    pub fn with_api_key(mut self, api_key_id: String) -> Self {
        self.api_key_id = Some(api_key_id);
        self
    }
    
    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }
    
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }
    
    pub fn with_rate_limit_key(mut self, key: String) -> Self {
        self.rate_limit_key = Some(key);
        self
    }
    
    pub fn with_previous_requests(mut self, requests: Vec<PreviousRequest>) -> Self {
        self.previous_requests = requests;
        self
    }
    
    /// Get the request frequency over the last N minutes
    pub fn request_frequency(&self, minutes: i64) -> f64 {
        let cutoff = Utc::now() - chrono::Duration::minutes(minutes);
        let recent_requests = self
            .previous_requests
            .iter()
            .filter(|req| req.timestamp > cutoff)
            .count();
        
        recent_requests as f64 / minutes as f64
    }
    
    /// Check if this appears to be an automated request
    pub fn appears_automated(&self) -> bool {
        // Check for common bot user agents
        if let Some(ua) = &self.user_agent {
            let bot_indicators = [
                "bot", "crawler", "spider", "scraper", "curl", "wget",
                "python", "java", "go-http", "okhttp", "axios"
            ];
            
            let ua_lower = ua.to_lowercase();
            if bot_indicators.iter().any(|&indicator| ua_lower.contains(indicator)) {
                return true;
            }
        }
        
        // Check for high request frequency
        if self.request_frequency(5) > 10.0 {
            return true;
        }
        
        // Check for missing common headers
        let common_headers = ["accept", "accept-language", "accept-encoding"];
        let missing_headers = common_headers
            .iter()
            .filter(|&&header| !self.headers.contains_key(header))
            .count();
        
        missing_headers >= 2
    }
    
    /// Get geographic information from IP (placeholder for actual GeoIP)
    pub fn get_country_code(&self) -> Option<String> {
        // This would integrate with a real GeoIP service
        // For now, return None to indicate unknown
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threat_level_from_score() {
        assert_eq!(ThreatLevel::from_score(0.05), ThreatLevel::None);
        assert_eq!(ThreatLevel::from_score(0.2), ThreatLevel::Low);
        assert_eq!(ThreatLevel::from_score(0.5), ThreatLevel::Medium);
        assert_eq!(ThreatLevel::from_score(0.7), ThreatLevel::High);
        assert_eq!(ThreatLevel::from_score(0.9), ThreatLevel::Critical);
    }

    #[test]
    fn test_threat_score_combination() {
        let scores = vec![
            ThreatScore::new("analyzer1".to_string(), 0.3, 0.8),
            ThreatScore::new("analyzer2".to_string(), 0.7, 0.9),
        ];
        
        let combined = ThreatScore::combine_scores(scores, None);
        assert_eq!(combined.score, 0.5); // (0.3 + 0.7) / 2
        assert_eq!(combined.confidence, 0.85); // (0.8 + 0.9) / 2
    }

    #[test]
    fn test_request_context_automation_detection() {
        let context = RequestContext::new(
            "192.168.1.1".to_string(),
            "/api/test".to_string(),
            "GET".to_string(),
        )
        .with_user_agent("curl/7.68.0".to_string());
        
        assert!(context.appears_automated());
        
        let context2 = RequestContext::new(
            "192.168.1.1".to_string(),
            "/api/test".to_string(),
            "GET".to_string(),
        )
        .with_user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36".to_string())
        .with_header("accept".to_string(), "text/html".to_string())
        .with_header("accept-language".to_string(), "en-US,en;q=0.9".to_string());
        
        assert!(!context2.appears_automated());
    }

    #[test]
    fn test_threat_score_actionable() {
        let score = ThreatScore::new("test".to_string(), 0.7, 0.8);
        assert!(score.is_actionable(0.6));
        assert!(!score.is_actionable(0.8));
        
        let low_confidence_score = ThreatScore::new("test".to_string(), 0.7, 0.3);
        assert!(!low_confidence_score.is_actionable(0.6));
    }
}