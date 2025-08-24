use crate::security::threat_analyzer::{ThreatAnalyzer, ThreatScore, RequestContext};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct IpReputationAnalyzer {
    providers: Vec<Box<dyn IpReputationProvider>>,
    cache: Arc<RwLock<HashMap<String, CachedReputationResult>>>,
    config: IpReputationConfig,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpReputationConfig {
    pub cache_ttl_minutes: i64,
    pub max_cache_size: usize,
    pub timeout_seconds: u64,
    pub min_confidence_threshold: f64,
    pub provider_weights: HashMap<String, f64>,
    pub enable_local_reputation: bool,
    pub local_reputation_decay_hours: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationResult {
    pub ip_address: String,
    pub reputation_score: f64, // 0.0 = good, 1.0 = bad
    pub confidence: f64,
    pub categories: Vec<ThreatCategory>,
    pub provider: String,
    pub last_seen: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ThreatCategory {
    Malware,
    Botnet,
    Spam,
    Phishing,
    Scanner,
    Brute Force,
    DDoS,
    Tor,
    Proxy,
    VPN,
    Hosting,
    Unknown,
}

#[derive(Debug, Clone)]
struct CachedReputationResult {
    result: ReputationResult,
    cached_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct LocalReputationEntry {
    ip_address: String,
    threat_events: Vec<LocalThreatEvent>,
    reputation_score: f64,
    last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct LocalThreatEvent {
    timestamp: DateTime<Utc>,
    event_type: String,
    severity: f64,
    details: String,
}

#[async_trait]
pub trait IpReputationProvider: Send + Sync {
    async fn check_reputation(&self, ip_address: &str) -> Result<ReputationResult>;
    fn provider_name(&self) -> &str;
    fn is_available(&self) -> bool;
}

impl IpReputationAnalyzer {
    pub async fn new() -> Result<Self> {
        let config = IpReputationConfig::default();
        let mut providers: Vec<Box<dyn IpReputationProvider>> = Vec::new();

        // Add built-in providers
        providers.push(Box::new(LocalReputationProvider::new()));
        providers.push(Box::new(StaticThreatListProvider::new()));

        // In a real implementation, you would add external providers here:
        // providers.push(Box::new(VirusTotalProvider::new(api_key)));
        // providers.push(Box::new(AbuseIPDBProvider::new(api_key)));

        Ok(Self {
            providers,
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
            enabled: true,
        })
    }

    pub async fn with_config(config: IpReputationConfig) -> Result<Self> {
        let mut analyzer = Self::new().await?;
        analyzer.config = config;
        Ok(analyzer)
    }

    async fn check_cache(&self, ip_address: &str) -> Option<ReputationResult> {
        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(ip_address) {
            if cached.expires_at > Utc::now() {
                debug!(ip_address = ip_address, "IP reputation cache hit");
                return Some(cached.result.clone());
            }
        }
        None
    }

    async fn cache_result(&self, result: &ReputationResult) {
        let mut cache = self.cache.write().await;
        
        // Implement LRU eviction if cache is full
        if cache.len() >= self.config.max_cache_size {
            // Remove oldest entry
            if let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.cached_at)
                .map(|(key, _)| key.clone())
            {
                cache.remove(&oldest_key);
            }
        }

        let cached_entry = CachedReputationResult {
            result: result.clone(),
            cached_at: Utc::now(),
            expires_at: Utc::now() + Duration::minutes(self.config.cache_ttl_minutes),
        };

        cache.insert(result.ip_address.clone(), cached_entry);
    }

    async fn query_providers(&self, ip_address: &str) -> Vec<ReputationResult> {
        let mut results = Vec::new();
        let timeout = tokio::time::Duration::from_secs(self.config.timeout_seconds);

        for provider in &self.providers {
            if !provider.is_available() {
                continue;
            }

            match tokio::time::timeout(timeout, provider.check_reputation(ip_address)).await {
                Ok(Ok(result)) => {
                    debug!(
                        ip_address = ip_address,
                        provider = provider.provider_name(),
                        score = result.reputation_score,
                        "IP reputation check completed"
                    );
                    results.push(result);
                }
                Ok(Err(e)) => {
                    warn!(
                        ip_address = ip_address,
                        provider = provider.provider_name(),
                        error = %e,
                        "IP reputation check failed"
                    );
                }
                Err(_) => {
                    warn!(
                        ip_address = ip_address,
                        provider = provider.provider_name(),
                        timeout_seconds = self.config.timeout_seconds,
                        "IP reputation check timed out"
                    );
                }
            }
        }

        results
    }

    fn combine_reputation_results(&self, results: Vec<ReputationResult>) -> ReputationResult {
        if results.is_empty() {
            return ReputationResult {
                ip_address: "unknown".to_string(),
                reputation_score: 0.0,
                confidence: 0.0,
                categories: Vec::new(),
                provider: "none".to_string(),
                last_seen: None,
                metadata: HashMap::new(),
            };
        }

        if results.len() == 1 {
            return results.into_iter().next().unwrap();
        }

        // Weighted combination of results
        let mut total_weight = 0.0;
        let mut weighted_score = 0.0;
        let mut weighted_confidence = 0.0;
        let mut all_categories = Vec::new();
        let mut combined_metadata = HashMap::new();

        for result in &results {
            let weight = self
                .config
                .provider_weights
                .get(&result.provider)
                .copied()
                .unwrap_or(1.0);

            total_weight += weight;
            weighted_score += result.reputation_score * weight;
            weighted_confidence += result.confidence * weight;
            all_categories.extend(result.categories.clone());

            for (key, value) in &result.metadata {
                combined_metadata.insert(
                    format!("{}_{}", result.provider, key),
                    value.clone(),
                );
            }
        }

        if total_weight > 0.0 {
            weighted_score /= total_weight;
            weighted_confidence /= total_weight;
        }

        // Deduplicate categories
        all_categories.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
        all_categories.dedup();

        ReputationResult {
            ip_address: results[0].ip_address.clone(),
            reputation_score: weighted_score,
            confidence: weighted_confidence,
            categories: all_categories,
            provider: "combined".to_string(),
            last_seen: results
                .iter()
                .filter_map(|r| r.last_seen)
                .max(),
            metadata: combined_metadata,
        }
    }

    /// Record a threat event for local reputation tracking
    pub async fn record_threat_event(
        &self,
        ip_address: &str,
        event_type: &str,
        severity: f64,
        details: &str,
    ) -> Result<()> {
        if !self.config.enable_local_reputation {
            return Ok(());
        }

        // This would typically be stored in a persistent store
        // For now, we'll just log it
        info!(
            ip_address = ip_address,
            event_type = event_type,
            severity = severity,
            details = details,
            "Threat event recorded for local reputation"
        );

        Ok(())
    }

    /// Get reputation statistics
    pub async fn get_statistics(&self) -> IpReputationStatistics {
        let cache = self.cache.read().await;
        
        IpReputationStatistics {
            cache_size: cache.len(),
            cache_hit_rate: 0.0, // Would be tracked in real implementation
            providers_count: self.providers.len(),
            enabled_providers: self.providers.iter().filter(|p| p.is_available()).count(),
            total_queries: 0, // Would be tracked in real implementation
            average_response_time_ms: 0.0,
        }
    }
}

#[async_trait]
impl ThreatAnalyzer for IpReputationAnalyzer {
    async fn analyze(&self, context: &RequestContext) -> Result<ThreatScore> {
        if !self.enabled {
            return Ok(ThreatScore::new(
                "ip_reputation".to_string(),
                0.0,
                1.0,
            ).with_reason("IP reputation analyzer disabled".to_string()));
        }

        // Validate IP address
        if context.ip_address.parse::<IpAddr>().is_err() {
            return Ok(ThreatScore::new(
                "ip_reputation".to_string(),
                0.0,
                0.0,
            ).with_reason("Invalid IP address format".to_string()));
        }

        // Check cache first
        if let Some(cached_result) = self.check_cache(&context.ip_address).await {
            let threat_score = ThreatScore::new(
                "ip_reputation".to_string(),
                cached_result.reputation_score,
                cached_result.confidence,
            )
            .with_reason(format!(
                "IP reputation: {} (cached)",
                self.format_categories(&cached_result.categories)
            ))
            .with_metadata("provider".to_string(), serde_json::Value::String(cached_result.provider))
            .with_metadata("categories".to_string(), serde_json::to_value(&cached_result.categories)?);

            return Ok(threat_score);
        }

        // Query providers
        let results = self.query_providers(&context.ip_address).await;
        let combined_result = self.combine_reputation_results(results);

        // Cache the result
        self.cache_result(&combined_result).await;

        // Convert to threat score
        let mut threat_score = ThreatScore::new(
            "ip_reputation".to_string(),
            combined_result.reputation_score,
            combined_result.confidence,
        );

        if combined_result.reputation_score > self.config.min_confidence_threshold {
            threat_score = threat_score.with_reason(format!(
                "IP reputation: {}",
                self.format_categories(&combined_result.categories)
            ));
        } else {
            threat_score = threat_score.with_reason("IP reputation: clean".to_string());
        }

        threat_score = threat_score
            .with_metadata("provider".to_string(), serde_json::Value::String(combined_result.provider))
            .with_metadata("categories".to_string(), serde_json::to_value(&combined_result.categories)?);

        Ok(threat_score)
    }

    fn analyzer_id(&self) -> &str {
        "ip_reputation"
    }

    fn name(&self) -> &str {
        "IP Reputation Analyzer"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn update_config(&mut self, config: serde_json::Value) -> Result<()> {
        if let Ok(new_config) = serde_json::from_value::<IpReputationConfig>(config) {
            self.config = new_config;
            info!("IP reputation analyzer configuration updated");
        }
        Ok(())
    }
}

impl IpReputationAnalyzer {
    fn format_categories(&self, categories: &[ThreatCategory]) -> String {
        if categories.is_empty() {
            "unknown".to_string()
        } else {
            categories
                .iter()
                .map(|c| format!("{:?}", c))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
}

// Built-in providers

pub struct LocalReputationProvider {
    // In a real implementation, this would connect to a local database
}

impl LocalReputationProvider {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl IpReputationProvider for LocalReputationProvider {
    async fn check_reputation(&self, ip_address: &str) -> Result<ReputationResult> {
        // This would check local threat intelligence
        // For now, return a neutral result
        Ok(ReputationResult {
            ip_address: ip_address.to_string(),
            reputation_score: 0.0,
            confidence: 0.5,
            categories: Vec::new(),
            provider: "local".to_string(),
            last_seen: None,
            metadata: HashMap::new(),
        })
    }

    fn provider_name(&self) -> &str {
        "local"
    }

    fn is_available(&self) -> bool {
        true
    }
}

pub struct StaticThreatListProvider {
    threat_ips: HashMap<String, ReputationResult>,
}

impl StaticThreatListProvider {
    pub fn new() -> Self {
        let mut threat_ips = HashMap::new();
        
        // Add some example threat IPs (in real implementation, this would be loaded from a file)
        threat_ips.insert(
            "192.0.2.1".to_string(),
            ReputationResult {
                ip_address: "192.0.2.1".to_string(),
                reputation_score: 0.9,
                confidence: 0.95,
                categories: vec![ThreatCategory::Malware, ThreatCategory::Botnet],
                provider: "static_list".to_string(),
                last_seen: Some(Utc::now() - Duration::days(1)),
                metadata: HashMap::new(),
            },
        );

        Self { threat_ips }
    }
}

#[async_trait]
impl IpReputationProvider for StaticThreatListProvider {
    async fn check_reputation(&self, ip_address: &str) -> Result<ReputationResult> {
        if let Some(result) = self.threat_ips.get(ip_address) {
            Ok(result.clone())
        } else {
            Ok(ReputationResult {
                ip_address: ip_address.to_string(),
                reputation_score: 0.0,
                confidence: 0.8,
                categories: Vec::new(),
                provider: "static_list".to_string(),
                last_seen: None,
                metadata: HashMap::new(),
            })
        }
    }

    fn provider_name(&self) -> &str {
        "static_list"
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub struct IpReputationStatistics {
    pub cache_size: usize,
    pub cache_hit_rate: f64,
    pub providers_count: usize,
    pub enabled_providers: usize,
    pub total_queries: u64,
    pub average_response_time_ms: f64,
}

impl Default for IpReputationConfig {
    fn default() -> Self {
        Self {
            cache_ttl_minutes: 60,
            max_cache_size: 10000,
            timeout_seconds: 5,
            min_confidence_threshold: 0.5,
            provider_weights: HashMap::new(),
            enable_local_reputation: true,
            local_reputation_decay_hours: 24,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ip_reputation_analyzer() {
        let analyzer = IpReputationAnalyzer::new().await.unwrap();
        
        let context = RequestContext::new(
            "192.0.2.1".to_string(),
            "/api/test".to_string(),
            "GET".to_string(),
        );

        let result = analyzer.analyze(&context).await.unwrap();
        
        assert_eq!(result.analyzer_id, "ip_reputation");
        assert!(result.score >= 0.0 && result.score <= 1.0);
        assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
    }

    #[tokio::test]
    async fn test_static_threat_list_provider() {
        let provider = StaticThreatListProvider::new();
        
        // Test known threat IP
        let result = provider.check_reputation("192.0.2.1").await.unwrap();
        assert!(result.reputation_score > 0.5);
        assert!(!result.categories.is_empty());
        
        // Test clean IP
        let result = provider.check_reputation("8.8.8.8").await.unwrap();
        assert_eq!(result.reputation_score, 0.0);
        assert!(result.categories.is_empty());
    }

    #[test]
    fn test_threat_category_formatting() {
        let analyzer = IpReputationAnalyzer::new().await.unwrap();
        
        let categories = vec![ThreatCategory::Malware, ThreatCategory::Botnet];
        let formatted = analyzer.format_categories(&categories);
        assert!(formatted.contains("Malware"));
        assert!(formatted.contains("Botnet"));
        
        let empty_categories = vec![];
        let formatted_empty = analyzer.format_categories(&empty_categories);
        assert_eq!(formatted_empty, "unknown");
    }
}