use crate::security::{
    response_engine::DefensiveAction,
    threat_analyzer::{RequestContext, ThreatScore},
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[derive(Debug)]
pub struct SiemIntegration {
    providers: Vec<SiemProviderType>,
    config: SiemConfig,
    event_queue: mpsc::UnboundedSender<SecurityEvent>,
}

#[derive(Debug, Clone)]
pub enum SiemProviderType {
    Syslog(SyslogProvider),
    Webhook(WebhookProvider),
    Splunk(SplunkProvider),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiemConfig {
    pub enabled: bool,
    pub batch_size: usize,
    pub flush_interval_seconds: u64,
    pub max_queue_size: usize,
    pub retry_attempts: u32,
    pub providers: Vec<SiemProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiemProviderConfig {
    pub name: String,
    pub provider_type: SiemProviderType,
    pub enabled: bool,
    pub config: HashMap<String, String>,
    pub event_filters: Vec<EventFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SiemProviderType {
    Splunk,
    ElasticSearch,
    ArcSight,
    QRadar,
    Sentinel,
    Syslog,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    pub field: String,
    pub operator: FilterOperator,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    GreaterThan,
    LessThan,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub event_id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: SecurityEventType,
    pub severity: SecurityEventSeverity,
    pub source: String,
    pub title: String,
    pub description: String,
    pub threat_score: f64,
    pub confidence: f64,
    pub actor: ActorInfo,
    pub target: TargetInfo,
    pub actions_taken: Vec<String>,
    pub raw_data: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEventType {
    ThreatDetected,
    AttackBlocked,
    AnomalousActivity,
    PolicyViolation,
    SystemAlert,
    UserActivity,
    NetworkEvent,
    DataAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEventSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    pub ip_address: String,
    pub user_agent: Option<String>,
    pub api_key_id: Option<String>,
    pub tenant_id: Option<String>,
    pub geolocation: Option<GeolocationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetInfo {
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub endpoint: String,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeolocationInfo {
    pub country: String,
    pub region: Option<String>,
    pub city: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

pub trait SiemProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn is_available(&self) -> bool;
    async fn send_event(&self, event: &SecurityEvent) -> Result<()>;
    async fn send_batch(&self, events: &[SecurityEvent]) -> Result<()>;
    async fn health_check(&self) -> Result<bool>;
}

impl SiemIntegration {
    pub async fn new(config: &SiemConfig) -> Result<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<SecurityEvent>();
        let mut providers: Vec<Box<dyn SiemProvider>> = Vec::new();

        // Initialize providers based on configuration
        for provider_config in &config.providers {
            if !provider_config.enabled {
                continue;
            }

            match provider_config.provider_type {
                SiemProviderType::Syslog => {
                    providers.push(Box::new(SyslogProvider::new(provider_config)?));
                }
                SiemProviderType::Webhook => {
                    providers.push(Box::new(WebhookProvider::new(provider_config)?));
                }
                SiemProviderType::Splunk => {
                    providers.push(Box::new(SplunkProvider::new(provider_config)?));
                }
                // Add other providers as needed
                _ => {
                    warn!(
                        provider_type = ?provider_config.provider_type,
                        "SIEM provider type not yet implemented"
                    );
                }
            }
        }

        let siem = Self {
            providers,
            config: config.clone(),
            event_queue: tx,
        };

        // Start background event processor
        let siem_clone = siem.clone();
        tokio::spawn(async move {
            siem_clone.process_events(rx).await;
        });

        info!(
            providers_count = siem.providers.len(),
            "SIEM integration initialized"
        );

        Ok(siem)
    }

    pub async fn send_security_event(
        &self,
        context: &RequestContext,
        threat_score: &ThreatScore,
        actions_taken: &[DefensiveAction],
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let event = self.create_security_event(context, threat_score, actions_taken);

        if let Err(e) = self.event_queue.send(event) {
            error!(error = %e, "Failed to queue security event for SIEM");
        }

        Ok(())
    }

    fn create_security_event(
        &self,
        context: &RequestContext,
        threat_score: &ThreatScore,
        actions_taken: &[DefensiveAction],
    ) -> SecurityEvent {
        let severity = match threat_score.level {
            crate::security::threat_analyzer::ThreatLevel::None => SecurityEventSeverity::Info,
            crate::security::threat_analyzer::ThreatLevel::Low => SecurityEventSeverity::Low,
            crate::security::threat_analyzer::ThreatLevel::Medium => SecurityEventSeverity::Medium,
            crate::security::threat_analyzer::ThreatLevel::High => SecurityEventSeverity::High,
            crate::security::threat_analyzer::ThreatLevel::Critical => SecurityEventSeverity::Critical,
        };

        let actor = ActorInfo {
            ip_address: context.ip_address.clone(),
            user_agent: context.user_agent.clone(),
            api_key_id: context.api_key_id.clone(),
            tenant_id: context.tenant_id.clone(),
            geolocation: None, // Would be populated from GeoIP service
        };

        let target = TargetInfo {
            resource_type: "api_endpoint".to_string(),
            resource_id: None,
            endpoint: context.endpoint.clone(),
            method: context.method.clone(),
        };

        let actions_descriptions: Vec<String> = actions_taken
            .iter()
            .map(|action| format!("{:?}", action))
            .collect();

        let mut raw_data = HashMap::new();
        raw_data.insert("threat_reasons".to_string(), serde_json::to_value(&threat_score.reasons).unwrap_or_default());
        raw_data.insert("threat_metadata".to_string(), serde_json::to_value(&threat_score.metadata).unwrap_or_default());
        raw_data.insert("request_headers".to_string(), serde_json::to_value(&context.headers).unwrap_or_default());

        let mut tags = vec![
            "ratewatch".to_string(),
            "threat_detection".to_string(),
            threat_score.analyzer_id.clone(),
        ];

        if let Some(tenant_id) = &context.tenant_id {
            tags.push(format!("tenant:{}", tenant_id));
        }

        SecurityEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type: SecurityEventType::ThreatDetected,
            severity,
            source: "ratewatch".to_string(),
            title: format!("Threat Detected: {}", threat_score.summary()),
            description: format!(
                "Threat detected from IP {} with score {:.2} (confidence: {:.2}). Reasons: {}",
                context.ip_address,
                threat_score.score,
                threat_score.confidence,
                threat_score.reasons.join(", ")
            ),
            threat_score: threat_score.score,
            confidence: threat_score.confidence,
            actor,
            target,
            actions_taken: actions_descriptions,
            raw_data,
            tags,
            correlation_id: context.correlation_id.to_string(),
        }
    }

    async fn process_events(&self, mut rx: mpsc::UnboundedReceiver<SecurityEvent>) {
        let mut event_batch = Vec::new();
        let mut flush_interval = tokio::time::interval(
            tokio::time::Duration::from_secs(self.config.flush_interval_seconds)
        );

        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Some(event) => {
                            event_batch.push(event);
                            
                            // Flush if batch is full
                            if event_batch.len() >= self.config.batch_size {
                                self.flush_events(&mut event_batch).await;
                            }
                        }
                        None => {
                            // Channel closed, flush remaining events and exit
                            if !event_batch.is_empty() {
                                self.flush_events(&mut event_batch).await;
                            }
                            break;
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    // Periodic flush
                    if !event_batch.is_empty() {
                        self.flush_events(&mut event_batch).await;
                    }
                }
            }
        }
    }

    async fn flush_events(&self, events: &mut Vec<SecurityEvent>) {
        if events.is_empty() {
            return;
        }

        debug!(events_count = events.len(), "Flushing security events to SIEM");

        for provider in &self.providers {
            if !provider.is_available() {
                continue;
            }

            // Filter events based on provider configuration
            let filtered_events: Vec<&SecurityEvent> = events
                .iter()
                .filter(|event| self.should_send_to_provider(event, provider.provider_name()))
                .collect();

            if filtered_events.is_empty() {
                continue;
            }

            // Convert to owned events for the provider
            let owned_events: Vec<SecurityEvent> = filtered_events.into_iter().cloned().collect();

            match provider.send_batch(&owned_events).await {
                Ok(_) => {
                    debug!(
                        provider = provider.provider_name(),
                        events_count = owned_events.len(),
                        "Successfully sent events to SIEM provider"
                    );
                }
                Err(e) => {
                    error!(
                        provider = provider.provider_name(),
                        events_count = owned_events.len(),
                        error = %e,
                        "Failed to send events to SIEM provider"
                    );
                }
            }
        }

        events.clear();
    }

    fn should_send_to_provider(&self, event: &SecurityEvent, provider_name: &str) -> bool {
        // Find provider configuration
        let provider_config = self
            .config
            .providers
            .iter()
            .find(|p| p.name == provider_name);

        if let Some(config) = provider_config {
            // Apply event filters
            for filter in &config.event_filters {
                if !self.apply_event_filter(event, filter) {
                    return false;
                }
            }
        }

        true
    }

    fn apply_event_filter(&self, event: &SecurityEvent, filter: &EventFilter) -> bool {
        let field_value = match filter.field.as_str() {
            "severity" => format!("{:?}", event.severity),
            "event_type" => format!("{:?}", event.event_type),
            "threat_score" => event.threat_score.to_string(),
            "source" => event.source.clone(),
            "ip_address" => event.actor.ip_address.clone(),
            _ => return true, // Unknown field, don't filter
        };

        match filter.operator {
            FilterOperator::Equals => field_value == filter.value,
            FilterOperator::NotEquals => field_value != filter.value,
            FilterOperator::Contains => field_value.contains(&filter.value),
            FilterOperator::NotContains => !field_value.contains(&filter.value),
            FilterOperator::GreaterThan => {
                if let (Ok(field_num), Ok(filter_num)) = (field_value.parse::<f64>(), filter.value.parse::<f64>()) {
                    field_num > filter_num
                } else {
                    false
                }
            }
            FilterOperator::LessThan => {
                if let (Ok(field_num), Ok(filter_num)) = (field_value.parse::<f64>(), filter.value.parse::<f64>()) {
                    field_num < filter_num
                } else {
                    false
                }
            }
            FilterOperator::Regex => {
                // Would implement regex matching here
                true
            }
        }
    }

    pub async fn health_check(&self) -> Result<HashMap<String, bool>> {
        let mut health_status = HashMap::new();

        for provider in &self.providers {
            let is_healthy = provider.health_check().await.unwrap_or(false);
            health_status.insert(provider.provider_name().to_string(), is_healthy);
        }

        Ok(health_status)
    }
}

// Built-in SIEM providers

pub struct SyslogProvider {
    name: String,
    facility: u8,
    severity: u8,
}

impl SyslogProvider {
    pub fn new(config: &SiemProviderConfig) -> Result<Self> {
        let facility = config
            .config
            .get("facility")
            .and_then(|f| f.parse().ok())
            .unwrap_or(16); // Local use 0

        let severity = config
            .config
            .get("severity")
            .and_then(|s| s.parse().ok())
            .unwrap_or(6); // Info

        Ok(Self {
            name: config.name.clone(),
            facility,
            severity,
        })
    }
}

impl SiemProvider for SyslogProvider {
    fn provider_name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn send_event(&self, event: &SecurityEvent) -> Result<()> {
        // In a real implementation, this would send to syslog
        info!(
            event_id = event.event_id,
            severity = ?event.severity,
            "SYSLOG: {}",
            event.description
        );
        Ok(())
    }

    async fn send_batch(&self, events: &[SecurityEvent]) -> Result<()> {
        for event in events {
            self.send_event(event).await?;
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

pub struct WebhookProvider {
    name: String,
    url: String,
    headers: HashMap<String, String>,
}

impl WebhookProvider {
    pub fn new(config: &SiemProviderConfig) -> Result<Self> {
        let url = config
            .config
            .get("url")
            .ok_or_else(|| anyhow::anyhow!("Webhook URL not configured"))?
            .clone();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        // Add custom headers from config
        for (key, value) in &config.config {
            if key.starts_with("header_") {
                let header_name = key.strip_prefix("header_").unwrap();
                headers.insert(header_name.to_string(), value.clone());
            }
        }

        Ok(Self {
            name: config.name.clone(),
            url,
            headers,
        })
    }
}

impl SiemProvider for WebhookProvider {
    fn provider_name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn send_event(&self, event: &SecurityEvent) -> Result<()> {
        // In a real implementation, this would make HTTP request
        debug!(
            url = self.url,
            event_id = event.event_id,
            "WEBHOOK: Sending security event"
        );
        Ok(())
    }

    async fn send_batch(&self, events: &[SecurityEvent]) -> Result<()> {
        // In a real implementation, this would send batch HTTP request
        debug!(
            url = self.url,
            events_count = events.len(),
            "WEBHOOK: Sending security event batch"
        );
        Ok(())
    }

    async fn health_check(&self) -> Result<bool> {
        // In a real implementation, this would check webhook endpoint
        Ok(true)
    }
}

pub struct SplunkProvider {
    name: String,
    hec_url: String,
    token: String,
}

impl SplunkProvider {
    pub fn new(config: &SiemProviderConfig) -> Result<Self> {
        let hec_url = config
            .config
            .get("hec_url")
            .ok_or_else(|| anyhow::anyhow!("Splunk HEC URL not configured"))?
            .clone();

        let token = config
            .config
            .get("token")
            .ok_or_else(|| anyhow::anyhow!("Splunk HEC token not configured"))?
            .clone();

        Ok(Self {
            name: config.name.clone(),
            hec_url,
            token,
        })
    }
}

impl SiemProvider for SplunkProvider {
    fn provider_name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn send_event(&self, event: &SecurityEvent) -> Result<()> {
        // In a real implementation, this would send to Splunk HEC
        debug!(
            hec_url = self.hec_url,
            event_id = event.event_id,
            "SPLUNK: Sending security event"
        );
        Ok(())
    }

    async fn send_batch(&self, events: &[SecurityEvent]) -> Result<()> {
        // In a real implementation, this would send batch to Splunk HEC
        debug!(
            hec_url = self.hec_url,
            events_count = events.len(),
            "SPLUNK: Sending security event batch"
        );
        Ok(())
    }

    async fn health_check(&self) -> Result<bool> {
        // In a real implementation, this would check Splunk HEC endpoint
        Ok(true)
    }
}

impl Default for SiemConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            batch_size: 100,
            flush_interval_seconds: 30,
            max_queue_size: 10000,
            retry_attempts: 3,
            providers: Vec::new(),
        }
    }
}

impl SiemProvider for SiemProviderType {
    async fn send_event(&self, event: &SecurityEvent) -> Result<()> {
        match self {
            SiemProviderType::Syslog(provider) => provider.send_event(event).await,
            SiemProviderType::Webhook(provider) => provider.send_event(event).await,
            SiemProviderType::Splunk(provider) => provider.send_event(event).await,
        }
    }

    async fn send_batch(&self, events: &[SecurityEvent]) -> Result<()> {
        match self {
            SiemProviderType::Syslog(provider) => provider.send_batch(events).await,
            SiemProviderType::Webhook(provider) => provider.send_batch(events).await,
            SiemProviderType::Splunk(provider) => provider.send_batch(events).await,
        }
    }

    async fn health_check(&self) -> Result<bool> {
        match self {
            SiemProviderType::Syslog(provider) => provider.health_check().await,
            SiemProviderType::Webhook(provider) => provider.health_check().await,
            SiemProviderType::Splunk(provider) => provider.health_check().await,
        }
    }

    fn provider_type(&self) -> &str {
        match self {
            SiemProviderType::Syslog(provider) => provider.provider_type(),
            SiemProviderType::Webhook(provider) => provider.provider_type(),
            SiemProviderType::Splunk(provider) => provider.provider_type(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_event_creation() {
        let context = crate::security::threat_analyzer::RequestContext::new(
            "192.168.1.1".to_string(),
            "/api/test".to_string(),
            "GET".to_string(),
        );

        let threat_score = crate::security::threat_analyzer::ThreatScore::new(
            "test".to_string(),
            0.7,
            0.8,
        );

        let siem = SiemIntegration {
            providers: Vec::new(),
            config: SiemConfig::default(),
            event_queue: tokio::sync::mpsc::unbounded_channel().0,
        };

        let event = siem.create_security_event(&context, &threat_score, &[]);

        assert_eq!(event.actor.ip_address, "192.168.1.1");
        assert_eq!(event.target.endpoint, "/api/test");
        assert_eq!(event.threat_score, 0.7);
        assert!(matches!(event.severity, SecurityEventSeverity::High));
    }

    #[test]
    fn test_event_filter() {
        let event = SecurityEvent {
            event_id: "test".to_string(),
            timestamp: Utc::now(),
            event_type: SecurityEventType::ThreatDetected,
            severity: SecurityEventSeverity::High,
            source: "ratewatch".to_string(),
            title: "Test".to_string(),
            description: "Test event".to_string(),
            threat_score: 0.8,
            confidence: 0.9,
            actor: ActorInfo {
                ip_address: "192.168.1.1".to_string(),
                user_agent: None,
                api_key_id: None,
                tenant_id: None,
                geolocation: None,
            },
            target: TargetInfo {
                resource_type: "api".to_string(),
                resource_id: None,
                endpoint: "/test".to_string(),
                method: "GET".to_string(),
            },
            actions_taken: Vec::new(),
            raw_data: HashMap::new(),
            tags: Vec::new(),
            correlation_id: "test".to_string(),
        };

        let siem = SiemIntegration {
            providers: Vec::new(),
            config: SiemConfig::default(),
            event_queue: tokio::sync::mpsc::unbounded_channel().0,
        };

        let filter = EventFilter {
            field: "severity".to_string(),
            operator: FilterOperator::Equals,
            value: "High".to_string(),
        };

        assert!(siem.apply_event_filter(&event, &filter));

        let filter2 = EventFilter {
            field: "threat_score".to_string(),
            operator: FilterOperator::GreaterThan,
            value: "0.5".to_string(),
        };

        assert!(siem.apply_event_filter(&event, &filter2));
    }
}