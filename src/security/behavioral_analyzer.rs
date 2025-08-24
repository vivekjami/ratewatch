use crate::security::threat_analyzer::{ThreatAnalyzer, ThreatScore, RequestContext};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct BehaviorAnalyzer {
    redis_client: Client,
    config: BehaviorAnalysisConfig,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorAnalysisConfig {
    pub analysis_window_minutes: i64,
    pub min_requests_for_analysis: u32,
    pub anomaly_threshold: f64,
    pub pattern_weights: HashMap<String, f64>,
    pub enable_ml_detection: bool,
    pub learning_period_hours: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorPattern {
    pub pattern_type: PatternType,
    pub confidence: f64,
    pub description: String,
    pub risk_score: f64,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternType {
    RapidRequests,
    UnusualEndpoints,
    SuspiciousUserAgent,
    TimeBasedAnomaly,
    RequestSizeAnomaly,
    GeographicAnomaly,
    SessionAnomaly,
    ErrorRateAnomaly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorMetrics {
    pub request_frequency: f64,
    pub unique_endpoints: usize,
    pub error_rate: f64,
    pub average_response_time: f64,
    pub request_size_variance: f64,
    pub time_distribution_entropy: f64,
    pub user_agent_consistency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BehaviorProfile {
    pub ip_address: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub request_count: u64,
    pub endpoints: HashMap<String, u32>,
    pub user_agents: HashMap<String, u32>,
    pub hourly_distribution: [u32; 24],
    pub error_count: u32,
    pub total_response_time: u64,
    pub request_sizes: Vec<u32>,
}

impl BehaviorAnalyzer {
    pub async fn new(redis_client: Client) -> Result<Self> {
        let config = BehaviorAnalysisConfig::default();
        
        Ok(Self {
            redis_client,
            config,
            enabled: true,
        })
    }

    pub async fn with_config(redis_client: Client, config: BehaviorAnalysisConfig) -> Result<Self> {
        Ok(Self {
            redis_client,
            config,
            enabled: true,
        })
    }

    async fn get_behavior_profile(&self, ip_address: &str) -> Result<Option<BehaviorProfile>> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("behavior:profile:{}", ip_address);
        
        let profile_data: Option<String> = conn.get(&key).await?;
        
        if let Some(data) = profile_data {
            match serde_json::from_str::<BehaviorProfile>(&data) {
                Ok(profile) => Ok(Some(profile)),
                Err(e) => {
                    warn!(
                        ip_address = ip_address,
                        error = %e,
                        "Failed to deserialize behavior profile"
                    );
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    async fn update_behavior_profile(&self, context: &RequestContext) -> Result<()> {
        let mut conn = self.redis_client.get_async_connection().await?;
        let key = format!("behavior:profile:{}", context.ip_address);
        
        // Get existing profile or create new one
        let mut profile = self.get_behavior_profile(&context.ip_address)
            .await?
            .unwrap_or_else(|| BehaviorProfile {
                ip_address: context.ip_address.clone(),
                first_seen: context.timestamp,
                last_seen: context.timestamp,
                request_count: 0,
                endpoints: HashMap::new(),
                user_agents: HashMap::new(),
                hourly_distribution: [0; 24],
                error_count: 0,
                total_response_time: 0,
                request_sizes: Vec::new(),
            });

        // Update profile with current request
        profile.last_seen = context.timestamp;
        profile.request_count += 1;
        
        // Update endpoint usage
        *profile.endpoints.entry(context.endpoint.clone()).or_insert(0) += 1;
        
        // Update user agent usage
        if let Some(ua) = &context.user_agent {
            *profile.user_agents.entry(ua.clone()).or_insert(0) += 1;
        }
        
        // Update hourly distribution
        let hour = context.timestamp.hour() as usize;
        if hour < 24 {
            profile.hourly_distribution[hour] += 1;
        }
        
        // Update error count (would need status code from context)
        // For now, we'll estimate based on previous requests
        if let Some(prev_req) = context.previous_requests.last() {
            if prev_req.status_code >= 400 {
                profile.error_count += 1;
            }
            profile.total_response_time += prev_req.response_time_ms;
        }

        // Store updated profile
        let profile_data = serde_json::to_string(&profile)?;
        conn.set_ex(&key, &profile_data, 86400 * 7).await?; // 7 days TTL

        Ok(())
    }

    async fn analyze_patterns(&self, context: &RequestContext, profile: &BehaviorProfile) -> Vec<BehaviorPattern> {
        let mut patterns = Vec::new();
        
        // Analyze request frequency
        patterns.extend(self.analyze_request_frequency(context, profile).await);
        
        // Analyze endpoint usage
        patterns.extend(self.analyze_endpoint_patterns(context, profile).await);
        
        // Analyze user agent patterns
        patterns.extend(self.analyze_user_agent_patterns(context, profile).await);
        
        // Analyze time-based patterns
        patterns.extend(self.analyze_time_patterns(context, profile).await);
        
        // Analyze error patterns
        patterns.extend(self.analyze_error_patterns(context, profile).await);

        patterns
    }

    async fn analyze_request_frequency(&self, context: &RequestContext, profile: &BehaviorProfile) -> Vec<BehaviorPattern> {
        let mut patterns = Vec::new();
        
        // Calculate requests per minute over the analysis window
        let window_start = context.timestamp - Duration::minutes(self.config.analysis_window_minutes);
        let recent_requests = context.previous_requests
            .iter()
            .filter(|req| req.timestamp > window_start)
            .count() as f64;
        
        let requests_per_minute = recent_requests / self.config.analysis_window_minutes as f64;
        
        // Define thresholds based on historical behavior
        let avg_requests_per_minute = profile.request_count as f64 / 
            (profile.last_seen - profile.first_seen).num_minutes().max(1) as f64;
        
        let frequency_ratio = if avg_requests_per_minute > 0.0 {
            requests_per_minute / avg_requests_per_minute
        } else {
            requests_per_minute
        };
        
        // Detect rapid request patterns
        if frequency_ratio > 5.0 && requests_per_minute > 10.0 {
            patterns.push(BehaviorPattern {
                pattern_type: PatternType::RapidRequests,
                confidence: (frequency_ratio / 10.0).min(1.0),
                description: format!(
                    "Rapid request pattern: {:.1} req/min ({}x normal rate)",
                    requests_per_minute, frequency_ratio
                ),
                risk_score: (frequency_ratio / 10.0).min(1.0),
                evidence: vec![
                    format!("Current rate: {:.1} req/min", requests_per_minute),
                    format!("Historical average: {:.1} req/min", avg_requests_per_minute),
                    format!("Frequency ratio: {:.1}x", frequency_ratio),
                ],
            });
        }
        
        patterns
    }

    async fn analyze_endpoint_patterns(&self, _context: &RequestContext, profile: &BehaviorProfile) -> Vec<BehaviorPattern> {
        let mut patterns = Vec::new();
        
        // Calculate endpoint diversity
        let unique_endpoints = profile.endpoints.len();
        let total_requests = profile.request_count;
        
        if total_requests > 0 {
            let endpoint_entropy = self.calculate_entropy(&profile.endpoints);
            
            // Detect unusual endpoint access patterns
            if unique_endpoints > 50 && endpoint_entropy > 4.0 {
                patterns.push(BehaviorPattern {
                    pattern_type: PatternType::UnusualEndpoints,
                    confidence: (endpoint_entropy / 6.0).min(1.0),
                    description: format!(
                        "Unusual endpoint access pattern: {} unique endpoints with high entropy ({:.2})",
                        unique_endpoints, endpoint_entropy
                    ),
                    risk_score: (endpoint_entropy / 6.0).min(1.0),
                    evidence: vec![
                        format!("Unique endpoints: {}", unique_endpoints),
                        format!("Endpoint entropy: {:.2}", endpoint_entropy),
                        format!("Total requests: {}", total_requests),
                    ],
                });
            }
        }
        
        patterns
    }

    async fn analyze_user_agent_patterns(&self, context: &RequestContext, profile: &BehaviorProfile) -> Vec<BehaviorPattern> {
        let mut patterns = Vec::new();
        
        if let Some(current_ua) = &context.user_agent {
            // Check for suspicious user agents
            let suspicious_indicators = [
                "bot", "crawler", "spider", "scraper", "curl", "wget",
                "python", "java", "go-http", "okhttp", "axios", "scanner"
            ];
            
            let ua_lower = current_ua.to_lowercase();
            let suspicious_count = suspicious_indicators
                .iter()
                .filter(|&&indicator| ua_lower.contains(indicator))
                .count();
            
            if suspicious_count > 0 {
                patterns.push(BehaviorPattern {
                    pattern_type: PatternType::SuspiciousUserAgent,
                    confidence: (suspicious_count as f64 / 3.0).min(1.0),
                    description: format!("Suspicious user agent detected: {}", current_ua),
                    risk_score: (suspicious_count as f64 / 3.0).min(1.0),
                    evidence: vec![
                        format!("User agent: {}", current_ua),
                        format!("Suspicious indicators: {}", suspicious_count),
                    ],
                });
            }
            
            // Check for user agent inconsistency
            let ua_count = profile.user_agents.len();
            if ua_count > 5 && profile.request_count > 20 {
                let ua_entropy = self.calculate_entropy(&profile.user_agents);
                if ua_entropy > 2.0 {
                    patterns.push(BehaviorPattern {
                        pattern_type: PatternType::SuspiciousUserAgent,
                        confidence: (ua_entropy / 4.0).min(1.0),
                        description: format!(
                            "Inconsistent user agent usage: {} different UAs with entropy {:.2}",
                            ua_count, ua_entropy
                        ),
                        risk_score: (ua_entropy / 4.0).min(1.0),
                        evidence: vec![
                            format!("Unique user agents: {}", ua_count),
                            format!("User agent entropy: {:.2}", ua_entropy),
                        ],
                    });
                }
            }
        }
        
        patterns
    }

    async fn analyze_time_patterns(&self, context: &RequestContext, profile: &BehaviorProfile) -> Vec<BehaviorPattern> {
        let mut patterns = Vec::new();
        
        // Analyze hourly distribution
        let total_requests = profile.hourly_distribution.iter().sum::<u32>();
        if total_requests > 24 { // Need sufficient data
            let time_entropy = self.calculate_array_entropy(&profile.hourly_distribution);
            
            // Very low entropy indicates concentrated activity in specific hours
            if time_entropy < 2.0 {
                let current_hour = context.timestamp.hour() as usize;
                let current_hour_requests = profile.hourly_distribution[current_hour];
                let avg_hourly_requests = total_requests as f64 / 24.0;
                
                if current_hour_requests as f64 > avg_hourly_requests * 3.0 {
                    patterns.push(BehaviorPattern {
                        pattern_type: PatternType::TimeBasedAnomaly,
                        confidence: (1.0 - time_entropy / 3.0).max(0.0),
                        description: format!(
                            "Concentrated activity pattern: {}% of requests in hour {} (entropy: {:.2})",
                            (current_hour_requests as f64 / total_requests as f64 * 100.0),
                            current_hour,
                            time_entropy
                        ),
                        risk_score: (1.0 - time_entropy / 3.0).max(0.0),
                        evidence: vec![
                            format!("Time entropy: {:.2}", time_entropy),
                            format!("Current hour requests: {}", current_hour_requests),
                            format!("Average hourly requests: {:.1}", avg_hourly_requests),
                        ],
                    });
                }
            }
        }
        
        patterns
    }

    async fn analyze_error_patterns(&self, _context: &RequestContext, profile: &BehaviorProfile) -> Vec<BehaviorPattern> {
        let mut patterns = Vec::new();
        
        if profile.request_count > 10 {
            let error_rate = profile.error_count as f64 / profile.request_count as f64;
            
            // High error rate might indicate probing or attacks
            if error_rate > 0.3 {
                patterns.push(BehaviorPattern {
                    pattern_type: PatternType::ErrorRateAnomaly,
                    confidence: error_rate,
                    description: format!(
                        "High error rate: {:.1}% ({} errors out of {} requests)",
                        error_rate * 100.0,
                        profile.error_count,
                        profile.request_count
                    ),
                    risk_score: error_rate,
                    evidence: vec![
                        format!("Error count: {}", profile.error_count),
                        format!("Total requests: {}", profile.request_count),
                        format!("Error rate: {:.1}%", error_rate * 100.0),
                    ],
                });
            }
        }
        
        patterns
    }

    fn calculate_entropy(&self, distribution: &HashMap<String, u32>) -> f64 {
        let total: u32 = distribution.values().sum();
        if total == 0 {
            return 0.0;
        }
        
        let mut entropy = 0.0;
        for &count in distribution.values() {
            if count > 0 {
                let probability = count as f64 / total as f64;
                entropy -= probability * probability.log2();
            }
        }
        
        entropy
    }

    fn calculate_array_entropy(&self, distribution: &[u32]) -> f64 {
        let total: u32 = distribution.iter().sum();
        if total == 0 {
            return 0.0;
        }
        
        let mut entropy = 0.0;
        for &count in distribution {
            if count > 0 {
                let probability = count as f64 / total as f64;
                entropy -= probability * probability.log2();
            }
        }
        
        entropy
    }

    fn calculate_combined_risk_score(&self, patterns: &[BehaviorPattern]) -> f64 {
        if patterns.is_empty() {
            return 0.0;
        }
        
        // Use weighted combination based on pattern types
        let mut total_weight = 0.0;
        let mut weighted_score = 0.0;
        
        for pattern in patterns {
            let weight = self.config.pattern_weights
                .get(&format!("{:?}", pattern.pattern_type))
                .copied()
                .unwrap_or(1.0);
            
            total_weight += weight;
            weighted_score += pattern.risk_score * weight;
        }
        
        if total_weight > 0.0 {
            weighted_score / total_weight
        } else {
            0.0
        }
    }
}

#[async_trait]
impl ThreatAnalyzer for BehaviorAnalyzer {
    async fn analyze(&self, context: &RequestContext) -> Result<ThreatScore> {
        if !self.enabled {
            return Ok(ThreatScore::new(
                "behavior_analysis".to_string(),
                0.0,
                1.0,
            ).with_reason("Behavior analyzer disabled".to_string()));
        }

        // Update behavior profile
        if let Err(e) = self.update_behavior_profile(context).await {
            error!(
                ip_address = context.ip_address,
                error = %e,
                "Failed to update behavior profile"
            );
        }

        // Get current behavior profile
        let profile = match self.get_behavior_profile(&context.ip_address).await? {
            Some(profile) => profile,
            None => {
                return Ok(ThreatScore::new(
                    "behavior_analysis".to_string(),
                    0.0,
                    0.5,
                ).with_reason("No behavior profile available".to_string()));
            }
        };

        // Need minimum requests for meaningful analysis
        if profile.request_count < self.config.min_requests_for_analysis as u64 {
            return Ok(ThreatScore::new(
                "behavior_analysis".to_string(),
                0.0,
                0.3,
            ).with_reason("Insufficient data for behavior analysis".to_string()));
        }

        // Analyze behavior patterns
        let patterns = self.analyze_patterns(context, &profile).await;
        
        if patterns.is_empty() {
            return Ok(ThreatScore::new(
                "behavior_analysis".to_string(),
                0.0,
                0.8,
            ).with_reason("No suspicious behavior patterns detected".to_string()));
        }

        // Calculate combined risk score
        let risk_score = self.calculate_combined_risk_score(&patterns);
        let confidence = patterns.iter().map(|p| p.confidence).sum::<f64>() / patterns.len() as f64;
        
        let reasons: Vec<String> = patterns.iter().map(|p| p.description.clone()).collect();
        
        let mut threat_score = ThreatScore::new(
            "behavior_analysis".to_string(),
            risk_score,
            confidence,
        ).with_reasons(reasons);

        // Add metadata
        threat_score = threat_score
            .with_metadata("patterns_detected".to_string(), serde_json::Value::Number(patterns.len().into()))
            .with_metadata("request_count".to_string(), serde_json::Value::Number(profile.request_count.into()))
            .with_metadata("profile_age_hours".to_string(), serde_json::Value::Number(
                (profile.last_seen - profile.first_seen).num_hours().into()
            ));

        debug!(
            ip_address = context.ip_address,
            risk_score = risk_score,
            patterns_count = patterns.len(),
            "Behavior analysis completed"
        );

        Ok(threat_score)
    }

    fn analyzer_id(&self) -> &str {
        "behavior_analysis"
    }

    fn name(&self) -> &str {
        "Behavioral Analysis"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn update_config(&mut self, config: serde_json::Value) -> Result<()> {
        if let Ok(new_config) = serde_json::from_value::<BehaviorAnalysisConfig>(config) {
            self.config = new_config;
            info!("Behavior analyzer configuration updated");
        }
        Ok(())
    }
}

impl Default for BehaviorAnalysisConfig {
    fn default() -> Self {
        let mut pattern_weights = HashMap::new();
        pattern_weights.insert("RapidRequests".to_string(), 1.5);
        pattern_weights.insert("UnusualEndpoints".to_string(), 1.2);
        pattern_weights.insert("SuspiciousUserAgent".to_string(), 1.0);
        pattern_weights.insert("TimeBasedAnomaly".to_string(), 0.8);
        pattern_weights.insert("ErrorRateAnomaly".to_string(), 1.3);
        
        Self {
            analysis_window_minutes: 15,
            min_requests_for_analysis: 10,
            anomaly_threshold: 0.6,
            pattern_weights,
            enable_ml_detection: false,
            learning_period_hours: 24,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_entropy_calculation() {
        let analyzer = BehaviorAnalyzer {
            redis_client: redis::Client::open("redis://127.0.0.1:6379").unwrap(),
            config: BehaviorAnalysisConfig::default(),
            enabled: true,
        };

        // Test uniform distribution (high entropy)
        let mut uniform_dist = HashMap::new();
        uniform_dist.insert("a".to_string(), 25);
        uniform_dist.insert("b".to_string(), 25);
        uniform_dist.insert("c".to_string(), 25);
        uniform_dist.insert("d".to_string(), 25);
        
        let entropy = analyzer.calculate_entropy(&uniform_dist);
        assert!(entropy > 1.9); // Should be close to 2.0 for uniform 4-item distribution

        // Test skewed distribution (low entropy)
        let mut skewed_dist = HashMap::new();
        skewed_dist.insert("a".to_string(), 90);
        skewed_dist.insert("b".to_string(), 5);
        skewed_dist.insert("c".to_string(), 3);
        skewed_dist.insert("d".to_string(), 2);
        
        let entropy_skewed = analyzer.calculate_entropy(&skewed_dist);
        assert!(entropy_skewed < entropy); // Should be lower than uniform distribution
    }

    #[test]
    fn test_pattern_risk_calculation() {
        let analyzer = BehaviorAnalyzer {
            redis_client: redis::Client::open("redis://127.0.0.1:6379").unwrap(),
            config: BehaviorAnalysisConfig::default(),
            enabled: true,
        };

        let patterns = vec![
            BehaviorPattern {
                pattern_type: PatternType::RapidRequests,
                confidence: 0.8,
                description: "Test pattern".to_string(),
                risk_score: 0.7,
                evidence: vec![],
            },
            BehaviorPattern {
                pattern_type: PatternType::SuspiciousUserAgent,
                confidence: 0.6,
                description: "Test pattern 2".to_string(),
                risk_score: 0.5,
                evidence: vec![],
            },
        ];

        let combined_score = analyzer.calculate_combined_risk_score(&patterns);
        assert!(combined_score > 0.0 && combined_score <= 1.0);
    }
}