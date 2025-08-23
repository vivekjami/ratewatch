use crate::audit::audit_event::{AuditEvent, AuditEventType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditFilter {
    pub name: String,
    pub enabled: bool,
    pub filter_type: AuditFilterType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditFilterType {
    /// Filter events by event type
    EventType(Vec<AuditEventType>),
    /// Filter events by actor (user_id or api_key_id)
    Actor(Vec<String>),
    /// Filter events by resource type
    ResourceType(Vec<String>),
    /// Filter events by tenant
    Tenant(Vec<String>),
    /// Filter events containing sensitive data
    SensitiveData,
    /// Filter events by IP address patterns
    IpAddress(Vec<String>),
    /// Custom filter with a predicate function name
    Custom(String),
}

impl AuditFilter {
    pub fn new(name: String, filter_type: AuditFilterType) -> Self {
        Self {
            name,
            enabled: true,
            filter_type,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Check if this filter should exclude the given event
    pub fn should_filter(&self, event: &AuditEvent) -> bool {
        if !self.enabled {
            return false;
        }

        match &self.filter_type {
            AuditFilterType::EventType(types) => {
                types.contains(&event.event_type)
            }
            AuditFilterType::Actor(actors) => {
                if let Some(user_id) = &event.actor.user_id {
                    if actors.contains(user_id) {
                        return true;
                    }
                }
                if let Some(api_key_id) = &event.actor.api_key_id {
                    if actors.contains(api_key_id) {
                        return true;
                    }
                }
                false
            }
            AuditFilterType::ResourceType(resource_types) => {
                resource_types.contains(&event.resource.resource_type)
            }
            AuditFilterType::Tenant(tenants) => {
                if let Some(tenant_id) = &event.tenant_id {
                    tenants.contains(tenant_id)
                } else {
                    false
                }
            }
            AuditFilterType::SensitiveData => {
                event.contains_sensitive_data()
            }
            AuditFilterType::IpAddress(ip_patterns) => {
                if let Some(ip) = &event.actor.ip_address {
                    ip_patterns.iter().any(|pattern| {
                        // Simple pattern matching - could be enhanced with regex
                        if pattern.ends_with('*') {
                            let prefix = &pattern[..pattern.len() - 1];
                            ip.starts_with(prefix)
                        } else {
                            ip == pattern
                        }
                    })
                } else {
                    false
                }
            }
            AuditFilterType::Custom(_function_name) => {
                // Custom filters would need to be implemented based on specific requirements
                // For now, we don't filter custom events
                false
            }
        }
    }
}

/// A collection of audit filters that can be applied to events
#[derive(Debug, Clone)]
pub struct AuditFilterSet {
    filters: Vec<AuditFilter>,
}

impl AuditFilterSet {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn add_filter(mut self, filter: AuditFilter) -> Self {
        self.filters.push(filter);
        self
    }

    pub fn with_filters(filters: Vec<AuditFilter>) -> Self {
        Self { filters }
    }

    /// Check if any filter in the set would exclude this event
    pub fn should_filter(&self, event: &AuditEvent) -> bool {
        self.filters.iter().any(|filter| filter.should_filter(event))
    }

    /// Get all enabled filters
    pub fn enabled_filters(&self) -> Vec<&AuditFilter> {
        self.filters.iter().filter(|f| f.enabled).collect()
    }

    /// Enable or disable a filter by name
    pub fn set_filter_enabled(&mut self, name: &str, enabled: bool) {
        if let Some(filter) = self.filters.iter_mut().find(|f| f.name == name) {
            filter.enabled = enabled;
        }
    }
}

impl Default for AuditFilterSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Predefined filter configurations for common use cases
impl AuditFilter {
    /// Filter to exclude health check events
    pub fn health_check_filter() -> Self {
        Self::new(
            "health_check".to_string(),
            AuditFilterType::ResourceType(vec!["health".to_string()]),
        )
    }

    /// Filter to exclude events from system actors
    pub fn system_actor_filter() -> Self {
        Self::new(
            "system_actors".to_string(),
            AuditFilterType::Actor(vec![
                "system".to_string(),
                "health-check".to_string(),
                "monitoring".to_string(),
            ]),
        )
    }

    /// Filter to exclude events with sensitive data (for general logging)
    pub fn sensitive_data_filter() -> Self {
        Self::new(
            "sensitive_data".to_string(),
            AuditFilterType::SensitiveData,
        )
    }

    /// Filter to exclude internal IP addresses
    pub fn internal_ip_filter() -> Self {
        Self::new(
            "internal_ips".to_string(),
            AuditFilterType::IpAddress(vec![
                "127.*".to_string(),
                "10.*".to_string(),
                "192.168.*".to_string(),
                "172.16.*".to_string(),
                "172.17.*".to_string(),
                "172.18.*".to_string(),
                "172.19.*".to_string(),
                "172.20.*".to_string(),
                "172.21.*".to_string(),
                "172.22.*".to_string(),
                "172.23.*".to_string(),
                "172.24.*".to_string(),
                "172.25.*".to_string(),
                "172.26.*".to_string(),
                "172.27.*".to_string(),
                "172.28.*".to_string(),
                "172.29.*".to_string(),
                "172.30.*".to_string(),
                "172.31.*".to_string(),
            ]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::audit_event::{ActorInfo, AuditOutcome, ResourceInfo};

    #[test]
    fn test_event_type_filter() {
        let filter = AuditFilter::new(
            "test".to_string(),
            AuditFilterType::EventType(vec![AuditEventType::Authentication]),
        );

        let event = AuditEvent::new(
            AuditEventType::Authentication,
            ActorInfo::new(),
            ResourceInfo::new("test".to_string()),
            "login".to_string(),
            AuditOutcome::Success,
        );

        assert!(filter.should_filter(&event));

        let event2 = AuditEvent::new(
            AuditEventType::DataAccess,
            ActorInfo::new(),
            ResourceInfo::new("test".to_string()),
            "read".to_string(),
            AuditOutcome::Success,
        );

        assert!(!filter.should_filter(&event2));
    }

    #[test]
    fn test_disabled_filter() {
        let filter = AuditFilter::new(
            "test".to_string(),
            AuditFilterType::EventType(vec![AuditEventType::Authentication]),
        ).disabled();

        let event = AuditEvent::new(
            AuditEventType::Authentication,
            ActorInfo::new(),
            ResourceInfo::new("test".to_string()),
            "login".to_string(),
            AuditOutcome::Success,
        );

        assert!(!filter.should_filter(&event));
    }

    #[test]
    fn test_filter_set() {
        let mut filter_set = AuditFilterSet::new()
            .add_filter(AuditFilter::health_check_filter())
            .add_filter(AuditFilter::system_actor_filter());

        let event = AuditEvent::new(
            AuditEventType::SystemEvent,
            ActorInfo::new(),
            ResourceInfo::new("health".to_string()),
            "check".to_string(),
            AuditOutcome::Success,
        );

        assert!(filter_set.should_filter(&event));

        filter_set.set_filter_enabled("health_check", false);
        assert!(!filter_set.should_filter(&event));
    }
}