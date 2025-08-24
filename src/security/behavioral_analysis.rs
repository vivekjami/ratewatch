use crate::security::threat_analyzer::{ThreatAnalyzer, ThreatScore, RequestContext, AnalyzerHealth, HealthStatus};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc, Duration};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorPattern {
    pub pattern_id: String,
    pub name: String,
    pub description: String,
    pub indicators: Vec<BehaviorIndicator>,
    pub severity: f64,  // 0.0 to 1.0
    pub confidence_threshold: f64,
    pub time_window_minutes: i64,
    pub min_occurrences: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorIndicator {
    pub indicator_type: IndicatorType,
    pub value: String,
    pub weight: f64,
    pub comparison: ComparisonType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IndicatorType {
    RequestRate,
    EndpointPattern,
    UserAgent,
    HeaderPattern,
    ResponseTime,
    ErrorRate,
    PayloadSize,
    RequestMethod,
    TimeOfDay,
    Geographic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ComparisonType {
    GreaterThan,
    LessThan,
    Equals,
    Contains,
    Regex,
    Range,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyScore {
    pub score: f64,
    pub anomaly_type: AnomalyType,
    pub baseline_value: f64,
    pub current_value: f64,
    pub deviation: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnomalyType {
    RequestRateSpike,
    UnusualEndpoints,
    SuspiciousUserAgent,
    TimeBasedAnomaly,
    GeographicAnomaly,
    PayloadAnomaly,
    ErrorRateSpike,
    ResponseTimeAnomaly,
}

pub struct BehaviorAnalyzer {
    redis: redis::Client,
    patterns: Arc<RwLock<Vec<BehaviorPattern>>>,
    config: Arc<RwLock<BehaviorAnalysisConfig>>,
    metrics: Arc<RwLock<BehaviorMetrics>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorAnalysisConfig {
    pub enabled: bool,
    pub learning_mode: bool,
    pub baseline_days: u32,
    pub anomaly_threshold: f64,
    pub max_patterns: usize,
    pub pattern_update_interval_hours: u64,
}

#[derive(Debug, Clone)]
struct BehaviorMetrics {
    pub patterns_matched: u64,
    pub anomalies_detected: u64,
    pub false_positives: u64,
    pub analysis_time_ms: f64,
    pub baseline_updates: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserBehaviorProfile {
    pub user_id: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub request_patterns: HashMap<String, RequestPattern>,
    pub typical_hours: Vec<u8>, // Hours of day when user is typically active
    pub typical_endpoints: Vec<String>,
    pub average_request_rate: f64,
    pub geographic_locations: Vec<String>,
    pub user_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RequestPattern {
    pub endpoint: String,
    pub method: String,
    pub frequency: u64,
    pub average_response_time: f64,
    pub typical_payload_size: f64,
    pub error_rate: f64,
}

impl BehaviorAnalyzer {
    pub async fn new(redis: redis::Client) -> Result<Self> {
        let patterns = Self::load_default_patterns();
        let config = BehaviorAnalysisConfig::default();
        
        Ok(Self {
            redis,
            patterns: Arc::new(RwLock::new(patterns)),
            config: Arc::new(RwLock::new(config)),
            metrics: Arc::new(RwLock::new(BehaviorMetrics::new())),
        })
    }
    
    fn load_default_patterns() -> Vec<BehaviorPattern> {
        vec![
            // High request rate pattern
            BehaviorPattern {
                pattern_id: "high_request_rate".to_string(),
                name: "High Request Rate".to_string(),
                description: "Unusually high number of requests in short time".to_string(),
                indicators: vec![
                    BehaviorIndicator {
                        indicator_type: IndicatorType::RequestRate,
                        value: "100".to_string(), // requests per minute
                        weight: 1.0,
                        comparison: ComparisonType::GreaterThan,
                    }
                ],
                severity: 0.7,
                confidence_threshold: 0.8,
                time_window_minutes: 5,
                min_occurrences: 1,
            },
            
            // Endpoint scanning pattern
            BehaviorPattern {
                pattern_id: "endpoint_scanning".to_string(),
                name: "Endpoint Scanning".to_string(),
                description: "Accessing many different endpoints rapidly".to_string(),
                indicators: vec![
                    BehaviorIndicator {
                        indicator_type: IndicatorType::EndpointPattern,
                        value: "20".to_string(), // unique endpoints
                        weight: 0.8,
                        comparison: ComparisonType::GreaterThan,
                    },
                    BehaviorIndicator {
                        indicator_type: IndicatorType::ErrorRate,
                        value: "0.5".to_string(), // 50% error rate
                        weight: 0.6,
                        comparison: ComparisonType::GreaterThan,
                    }
                ],
                severity: 0.8,
                confidence_threshold: 0.7,
                time_window_minutes: 10,
                min_occurrences: 1,
            },
            
            // Suspicious user agent pattern
            BehaviorPattern {
                pattern_id: "suspicious_user_agent".to_string(),
                name: "Suspicious User Agent".to_string(),
                description: "User agent indicates automated tool or scanner".to_string(),
                indicators: vec![
                    BehaviorIndicator {
                        indicator_type: IndicatorType::UserAgent,
                        value: r"(?i)(bot|crawler|scanner|curl|wget|python|java)".to_string(),
                        weight: 0.6,
                        comparison: ComparisonType::Regex,
                    }
                ],
                severity: 0.4,
                confidence_threshold: 0.9,
                time_window_minutes: 1,
                min_occurrences: 1,
            },
            
            // Off-hours activity pattern
            BehaviorPattern {
                pattern_id: "off_hours_activity".to_string(),
                name: "Off-Hours Activity".to_string(),
                description: "Unusual activity during off-business hours".to_string(),
                indicators: vec![
                    BehaviorIndicator {
                        indicator_type: IndicatorType::TimeOfDay,
                        value: "22-06".to_string(), // 10 PM to 6 AM
                        weight: 0.3,
                        comparison: ComparisonType::Range,
                    },
                    BehaviorIndicator {
                        indicator_type: IndicatorType::RequestRate,
                        value: "50".to_string(),
                        weight: 0.7,
                        comparison: ComparisonType::GreaterThan,
                    }
                ],
                severity: 0.5,
                confidence_threshold: 0.6,
                time_window_minutes: 60,
                min_occurrences: 1,
            },
        ]
    }
    
    async fn get_user_profile(&self, user_id: &str) -> Result<Option<UserBehaviorProfile>> {
        let mut conn = self.redis.get_async_connection().await?;
        let key = format!("behavior_profile:{}", user_id);
        
        let profile_data: Option<String> = conn.get(&key).await?;
        
        if let Some(data) = profile_data {
            let profile: UserBehaviorProfile = serde_json::from_str(&data)?;
            Ok(Some(profile))
        } else {
            Ok(None)
        }
    }
    
    async fn update_user_profile(&self, context: &RequestContext) -> Result<()> {
        if let Some(user_id) = &context.api_key_id {
            let mut profile = self.get_user_profile(user_id).await?
                .unwrap_or_else(|| UserBehaviorProfile::new(user_id.clone()));
            
            // Update profile with current request
            profile.last_seen = context.timestamp;
            profile.typical_endpoints.push(context.endpoint.clone());
            
            if let Some(ua) = &context.user_agent {
                if !profile.user_agents.contains(ua) {
                    profile.user_agents.push(ua.clone());
                }
            }
            
            // Update request pattern
            let pattern_key = format!("{}:{}", context.method, context.endpoint);
            let pattern = profile.request_patterns.entry(pattern_key.clone())
                .or_insert_with(|| RequestPattern::new(context.endpoint.clone(), context.method.clone()));
            
            pattern.frequency += 1;
            
            // Save updated profile
            let mut conn = self.redis.get_async_connection().await?;
            let key = format!("behavior_profile:{}", user_id);
            let profile_data = serde_json::to_string(&profile)?;
            
            conn.set_ex(&key, profile_data, 86400 * 30).await?; // 30 days
        }
        
        Ok(())
    }
    
    async fn analyze_request_rate(&self, context: &RequestContext) -> Result<Vec<AnomalyScore>> {
        let mut anomalies = Vec::new();
        
        // Get recent request history
        let key = format!("request_rate:{}", context.ip_address);
        let mut conn = self.redis.get_async_connection().await?;
        
        // Count requests in last 5 minutes
        let five_min_ago = Utc::now() - Duration::minutes(5);
        let recent_requests: u64 = conn.zcount(&key, five_min_ago.timestamp(), Utc::now().timestamp()).await?;
        
        let request_rate = recent_requests as f64 / 5.0; // requests per minute
        
        // Get baseline (average over last 24 hours)
        let day_ago = Utc::now() - Duration::hours(24);
        let day_requests: u64 = conn.zcount(&key, day_ago.timestamp(), Utc::now().timestamp()).await?;
        let baseline_rate = day_requests as f64 / (24.0 * 60.0); // requests per minute
        
        if baseline_rate > 0.0 {
            let deviation = (request_rate - baseline_rate) / baseline_rate;
            
            if deviation > 2.0 { // More than 200% increase
                anomalies.push(AnomalyScore {
                    score: (deviation / 5.0).min(1.0), // Cap at 1.0
                    anomaly_type: AnomalyType::RequestRateSpike,
                    baseline_value: baseline_rate,
                    current_value: request_rate,
                    deviation,
                    confidence: 0.8,
                });
            }
        }
        
        // Record current request
        conn.zadd(&key, context.timestamp.timestamp(), context.correlation_id.to_string()).await?;
        conn.expire(&key, 86400).await?; // Keep for 24 hours
        
        Ok(anomalies)
    }
    
    async fn analyze_endpoint_patterns(&self, context: &RequestContext) -> Result<Vec<AnomalyScore>> {
        let mut anomalies = Vec::new();
        
        if let Some(user_id) = &context.api_key_id {
            let profile = self.get_user_profile(user_id).await?;
            
            if let Some(profile) = profile {
                // Check if this endpoint is typical for this user
                if !profile.typical_endpoints.contains(&context.endpoint) {
                    // Count unique endpoints accessed in last hour
                    let key = format!("endpoints:{}:{}", user_id, Utc::now().format("%Y%m%d%H"));
                    let mut conn = self.redis.get_async_connection().await?;
                    
                    conn.sadd(&key, &context.endpoint).await?;
                    conn.expire(&key, 3600).await?; // 1 hour
                    
                    let unique_endpoints: u64 = conn.scard(&key).await?;
                    
                    if unique_endpoints > 10 {
                        anomalies.push(AnomalyScore {
                            score: (unique_endpoints as f64 / 50.0).min(1.0),
                            anomaly_type: AnomalyType::UnusualEndpoints,
                            baseline_value: profile.typical_endpoints.len() as f64,
                            current_value: unique_endpoints as f64,
                            deviation: unique_endpoints as f64 - profile.typical_endpoints.len() as f64,
                            confidence: 0.7,
                        });
                    }
                }
            }
        }
        
        Ok(anomalies)
    }
    
    async fn analyze_time_patterns(&self, context: &RequestContext) -> Result<Vec<AnomalyScore>> {
        let mut anomalies = Vec::new();
        
        let hour = context.timestamp.hour();
        
        // Check if request is during typical off-hours (10 PM to 6 AM)
        if hour >= 22 || hour <= 6 {
            if let Some(user_id) = &context.api_key_id {
                let profile = self.get_user_profile(user_id).await?;
                
                if let Some(profile) = profile {
                    if !profile.typical_hours.contains(&(hour as u8)) {
                        anomalies.push(AnomalyScore {
                            score: 0.4,
                            anomaly_type: AnomalyType::TimeBasedAnomaly,
                            baseline_value: profile.typical_hours.len() as f64,
                            current_value: hour as f64,
                            deviation: 1.0,
                            confidence: 0.6,
                        });
                    }
                }
            }
        }
        
        Ok(anomalies)
    }
    
    async fn match_patterns(&self, context: &RequestContext) -> Result<Vec<(BehaviorPattern, f64)>> {
        let patterns = self.patterns.read().await;
        let mut matches = Vec::new();
        
        for pattern in patterns.iter() {
            let mut total_score = 0.0;
            let mut matched_indicators = 0;
            
            for indicator in &pattern.indicators {
                if let Some(score) = self.evaluate_indicator(indicator, context).await? {
                    total_score += score * indicator.weight;
                    matched_indicators += 1;
                }
            }
            
            if matched_indicators > 0 {
                let pattern_score = total_score / pattern.indicators.len() as f64;
                if pattern_score >= pattern.confidence_threshold {
                    matches.push((pattern.clone(), pattern_score));
                }
            }
        }
        
        Ok(matches)
    }
    
    async fn evaluate_indicator(&self, indicator: &BehaviorIndicator, context: &RequestContext) -> Result<Option<f64>> {
        match indicator.indicator_type {
            IndicatorType::RequestRate => {
                let rate = context.get_request_rate(5); // 5 minute window
                let threshold: f64 = indicator.value.parse().unwrap_or(0.0);
                
                match indicator.comparison {
                    ComparisonType::GreaterThan => {
                        if rate > threshold {
                            Ok(Some((rate / threshold).min(1.0)))
                        } else {
                            Ok(None)
                        }
                    }
                    _ => Ok(None),
                }
            }
            
            IndicatorType::UserAgent => {
                if let Some(ua) = &context.user_agent {
                    match indicator.comparison {
                        ComparisonType::Regex => {
                            if let Ok(regex) = regex::Regex::new(&indicator.value) {
                                if regex.is_match(ua) {
                                    Ok(Some(1.0))
                                } else {
                                    Ok(None)
                                }
                            } else {
                                Ok(None)
                            }
                        }
                        ComparisonType::Contains => {
                            if ua.to_lowercase().contains(&indicator.value.to_lowercase()) {
                                Ok(Some(1.0))
                            } else {
                                Ok(None)
                            }
                        }
                        _ => Ok(None),
                    }
                } else {
                    Ok(None)
                }
            }
            
            IndicatorType::TimeOfDay => {
                let hour = context.timestamp.hour();
                
                match indicator.comparison {
                    ComparisonType::Range => {
                        if let Some((start, end)) = self.parse_time_range(&indicator.value) {
                            if (start <= end && hour >= start && hour <= end) ||
                               (start > end && (hour >= start || hour <= end)) {
                                Ok(Some(1.0))
                            } else {
                                Ok(None)
                            }
                        } else {
                            Ok(None)
                        }
                    }
                    _ => Ok(None),
                }
            }
            
            _ => Ok(None), // Other indicators not implemented yet
        }
    }
    
    fn parse_time_range(&self, range: &str) -> Option<(u32, u32)> {
        let parts: Vec<&str> = range.split('-').collect();
        if parts.len() == 2 {
            if let (Ok(start), Ok(end)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                return Some((start, end));
            }
        }
        None
    }
}

#[async_trait]
impl ThreatAnalyzer for BehaviorAnalyzer {
    async fn analyze(&self, context: &RequestContext) -> Result<ThreatScore> {
        let start_time = std::time::Instant::now();
        
        // Update user profile
        if let Err(e) = self.update_user_profile(context).await {
            tracing::warn!("Failed to update user profile: {}", e);
        }
        
        let mut total_score = 0.0;
        let mut reasons = Vec::new();
        let mut metadata = HashMap::new();
        
        // Analyze request rate anomalies
        let rate_anomalies = self.analyze_request_rate(context).await?;
        for anomaly in &rate_anomalies {
            total_score += anomaly.score * 0.3;
            reasons.push(format!("Request rate anomaly: {:.1}x baseline", anomaly.current_value / anomaly.baseline_value));
        }
        
        // Analyze endpoint patterns
        let endpoint_anomalies = self.analyze_endpoint_patterns(context).await?;
        for anomaly in &endpoint_anomalies {
            total_score += anomaly.score * 0.2;
            reasons.push(format!("Unusual endpoint access pattern detected"));
        }
        
        // Analyze time patterns
        let time_anomalies = self.analyze_time_patterns(context).await?;
        for anomaly in &time_anomalies {
            total_score += anomaly.score * 0.1;
            reasons.push(format!("Off-hours activity detected"));
        }
        
        // Match behavior patterns
        let pattern_matches = self.match_patterns(context).await?;
        for (pattern, score) in &pattern_matches {
            total_score += score * pattern.severity * 0.4;
            reasons.push(format!("Matched pattern: {}", pattern.name));
        }
        
        // Update metrics
        let elapsed = start_time.elapsed().as_millis() as f64;
        {
            let mut metrics = self.metrics.write().await;
            metrics.patterns_matched += pattern_matches.len() as u64;
            if !rate_anomalies.is_empty() || !endpoint_anomalies.is_empty() || !time_anomalies.is_empty() {
                metrics.anomalies_detected += 1;
            }
            metrics.analysis_time_ms = (metrics.analysis_time_ms + elapsed) / 2.0; // Simple moving average
        }
        
        // Clamp score
        total_score = total_score.min(1.0);
        
        // Add metadata
        metadata.insert("anomalies".to_string(), serde_json::to_value(&rate_anomalies).unwrap_or_default());
        metadata.insert("pattern_matches".to_string(), serde_json::to_value(&pattern_matches).unwrap_or_default());
        
        let mut score = ThreatScore::new(
            self.analyzer_id().to_string(),
            total_score,
            reasons,
        );
        
        for (key, value) in metadata {
            score = score.with_metadata(key, value);
        }
        
        Ok(score)
    }
    
    fn analyzer_id(&self) -> &str {
        "behavior_analysis"
    }
    
    fn name(&self) -> &str {
        "Behavioral Analysis"
    }
    
    fn is_enabled(&self) -> bool {
        true
    }
    
    async fn update_config(&self, config: serde_json::Value) -> Result<()> {
        if let Ok(new_config) = serde_json::from_value::<BehaviorAnalysisConfig>(config) {
            let mut current_config = self.config.write().await;
            *current_config = new_config;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Invalid configuration format"))
        }
    }
    
    async fn health_check(&self) -> Result<AnalyzerHealth> {
        let metrics = self.metrics.read().await;
        
        let health = if metrics.analysis_time_ms > 500.0 {
            AnalyzerHealth::new(self.analyzer_id().to_string())
                .degraded("High analysis time".to_string())
        } else {
            AnalyzerHealth::new(self.analyzer_id().to_string())
                .healthy()
        };
        
        Ok(health.with_metrics(
            metrics.patterns_matched + metrics.anomalies_detected,
            0, // No explicit error tracking yet
            metrics.analysis_time_ms,
        ))
    }
}

impl UserBehaviorProfile {
    fn new(user_id: String) -> Self {
        Self {
            user_id,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            request_patterns: HashMap::new(),
            typical_hours: Vec::new(),
            typical_endpoints: Vec::new(),
            average_request_rate: 0.0,
            geographic_locations: Vec::new(),
            user_agents: Vec::new(),
        }
    }
}

impl RequestPattern {
    fn new(endpoint: String, method: String) -> Self {
        Self {
            endpoint,
            method,
            frequency: 0,
            average_response_time: 0.0,
            typical_payload_size: 0.0,
            error_rate: 0.0,
        }
    }
}

impl Default for BehaviorAnalysisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            learning_mode: true,
            baseline_days: 7,
            anomaly_threshold: 0.6,
            max_patterns: 100,
            pattern_update_interval_hours: 24,
        }
    }
}

impl BehaviorMetrics {
    fn new() -> Self {
        Self {
            patterns_matched: 0,
            anomalies_detected: 0,
            false_positives: 0,
            analysis_time_ms: 0.0,
            baseline_updates: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_behavior_pattern_creation() {
        let pattern = BehaviorPattern {
            pattern_id: "test".to_string(),
            name: "Test Pattern".to_string(),
            description: "Test".to_string(),
            indicators: vec![],
            severity: 0.5,
            confidence_threshold: 0.7,
            time_window_minutes: 10,
            min_occurrences: 1,
        };
        
        assert_eq!(pattern.pattern_id, "test");
        assert_eq!(pattern.severity, 0.5);
    }

    #[test]
    fn test_anomaly_score() {
        let anomaly = AnomalyScore {
            score: 0.8,
            anomaly_type: AnomalyType::RequestRateSpike,
            baseline_value: 10.0,
            current_value: 50.0,
            deviation: 4.0,
            confidence: 0.9,
        };
        
        assert_eq!(anomaly.score, 0.8);
        assert_eq!(anomaly.anomaly_type, AnomalyType::RequestRateSpike);
    }
}