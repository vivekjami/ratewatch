use super::{TenantManager, TenantConfig};
use super::resource_quota::{ResourceType, QuotaManager};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use anyhow::{Result, anyhow};

pub type TenantManagerState = Arc<Mutex<TenantManager>>;

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub tenant_config: TenantConfig,
    pub is_authenticated: bool,
}

pub async fn tenant_resolution_middleware(
    State(tenant_manager): State<TenantManagerState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let tenant_context = match resolve_tenant_from_request(&request, tenant_manager.clone()).await {
        Ok(context) => context,
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    // Check if tenant is active
    if !tenant_context.tenant_config.is_active() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    // Add tenant context to request extensions
    request.extensions_mut().insert(tenant_context);

    Ok(next.run(request).await)
}

pub async fn tenant_quota_middleware(
    State(tenant_manager): State<TenantManagerState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let tenant_context = request.extensions().get::<TenantContext>()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut manager = tenant_manager.lock().await;
    
    // Check if tenant can make API calls
    let can_consume = manager.quota_manager.can_consume_resource(
        tenant_context.tenant_id,
        ResourceType::ApiCalls,
        1,
        &tenant_context.tenant_config.quotas,
    ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !can_consume {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    // Increment API call counter
    if let Err(_) = manager.quota_manager.update_usage(
        tenant_context.tenant_id,
        ResourceType::ApiCalls,
        1,
    ).await {
        tracing::warn!("Failed to update API call usage for tenant {}", tenant_context.tenant_id);
    }

    let response = next.run(request).await;

    // Check for quota violations after request
    if let Ok(violations) = manager.check_quota_violations(tenant_context.tenant_id).await {
        if !violations.is_empty() {
            tracing::warn!(
                "Quota violations detected for tenant {}: {:?}",
                tenant_context.tenant_id,
                violations
            );
        }
    }

    Ok(response)
}

pub async fn tenant_rate_limit_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let tenant_context = request.extensions().get::<TenantContext>()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Check rate limits based on tenant configuration
    let rate_limits = &tenant_context.tenant_config.settings.rate_limits;
    
    // This is a simplified rate limiting check
    // In production, you'd want to use a proper rate limiting library
    // that tracks requests per tenant over time windows
    
    // For now, we'll just check concurrent requests
    // (This would need to be implemented with proper tracking)
    
    Ok(next.run(request).await)
}

pub async fn tenant_security_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let tenant_context = request.extensions().get::<TenantContext>()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let security_settings = &tenant_context.tenant_config.settings.security_settings;

    // Check IP whitelist if configured
    if !security_settings.ip_whitelist.is_empty() {
        if let Some(client_ip) = get_client_ip(&request) {
            if !security_settings.ip_whitelist.contains(&client_ip) {
                tracing::warn!(
                    "IP {} not in whitelist for tenant {}",
                    client_ip,
                    tenant_context.tenant_id
                );
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    // Check allowed origins for CORS
    if let Some(origin) = request.headers().get("origin") {
        if let Ok(origin_str) = origin.to_str() {
            if !security_settings.allowed_origins.contains(&"*".to_string()) &&
               !security_settings.allowed_origins.contains(&origin_str.to_string()) {
                tracing::warn!(
                    "Origin {} not allowed for tenant {}",
                    origin_str,
                    tenant_context.tenant_id
                );
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    Ok(next.run(request).await)
}

pub async fn tenant_feature_gate_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let tenant_context = request.extensions().get::<TenantContext>()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Extract feature requirement from request path or headers
    let required_feature = extract_required_feature(&request);
    
    if let Some(feature) = required_feature {
        if !tenant_context.tenant_config.can_access_feature(&feature) {
            tracing::warn!(
                "Tenant {} does not have access to feature {}",
                tenant_context.tenant_id,
                feature
            );
            return Err(StatusCode::FORBIDDEN);
        }
    }

    Ok(next.run(request).await)
}

async fn resolve_tenant_from_request(
    request: &Request,
    tenant_manager: TenantManagerState,
) -> Result<TenantContext> {
    // Try to resolve tenant from different sources
    
    // 1. From subdomain (e.g., tenant1.api.example.com)
    if let Some(tenant_id) = extract_tenant_from_subdomain(request) {
        let mut manager = tenant_manager.lock().await;
        if let Ok(config) = manager.get_tenant_config(tenant_id).await {
            return Ok(TenantContext {
                tenant_id,
                tenant_config: config,
                is_authenticated: false, // Would be set by auth middleware
            });
        }
    }

    // 2. From header (X-Tenant-ID or X-Tenant-Slug)
    if let Some(tenant_context) = extract_tenant_from_headers(request, tenant_manager.clone()).await? {
        return Ok(tenant_context);
    }

    // 3. From path parameter (/api/tenants/{tenant_id}/...)
    if let Some(tenant_id) = extract_tenant_from_path(request) {
        let mut manager = tenant_manager.lock().await;
        if let Ok(config) = manager.get_tenant_config(tenant_id).await {
            return Ok(TenantContext {
                tenant_id,
                tenant_config: config,
                is_authenticated: false,
            });
        }
    }

    Err(anyhow!("Could not resolve tenant from request"))
}

fn extract_tenant_from_subdomain(request: &Request) -> Option<Uuid> {
    request.headers()
        .get("host")
        .and_then(|host| host.to_str().ok())
        .and_then(|host_str| {
            // Extract subdomain from host like "tenant1.api.example.com"
            let parts: Vec<&str> = host_str.split('.').collect();
            if parts.len() >= 3 {
                // Try to parse first part as UUID
                Uuid::parse_str(parts[0]).ok()
            } else {
                None
            }
        })
}

async fn extract_tenant_from_headers(
    request: &Request,
    tenant_manager: TenantManagerState,
) -> Result<Option<TenantContext>> {
    let headers = request.headers();

    // Try X-Tenant-ID header
    if let Some(tenant_id_header) = headers.get("x-tenant-id") {
        if let Ok(tenant_id_str) = tenant_id_header.to_str() {
            if let Ok(tenant_id) = Uuid::parse_str(tenant_id_str) {
                let mut manager = tenant_manager.lock().await;
                if let Ok(config) = manager.get_tenant_config(tenant_id).await {
                    return Ok(Some(TenantContext {
                        tenant_id,
                        tenant_config: config,
                        is_authenticated: false,
                    }));
                }
            }
        }
    }

    // Try X-Tenant-Slug header
    if let Some(tenant_slug_header) = headers.get("x-tenant-slug") {
        if let Ok(tenant_slug) = tenant_slug_header.to_str() {
            let mut manager = tenant_manager.lock().await;
            if let Ok(Some(config)) = manager.get_tenant_by_slug(tenant_slug).await {
                return Ok(Some(TenantContext {
                    tenant_id: config.id,
                    tenant_config: config,
                    is_authenticated: false,
                }));
            }
        }
    }

    Ok(None)
}

fn extract_tenant_from_path(request: &Request) -> Option<Uuid> {
    let path = request.uri().path();
    
    // Look for patterns like /api/tenants/{tenant_id}/...
    let path_segments: Vec<&str> = path.split('/').collect();
    
    for (i, segment) in path_segments.iter().enumerate() {
        if *segment == "tenants" && i + 1 < path_segments.len() {
            if let Ok(tenant_id) = Uuid::parse_str(path_segments[i + 1]) {
                return Some(tenant_id);
            }
        }
    }
    
    None
}

fn get_client_ip(request: &Request) -> Option<String> {
    // Check various headers for client IP
    let headers = request.headers();
    
    // X-Forwarded-For (most common)
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(xff_str) = xff.to_str() {
            // Take the first IP in the chain
            return xff_str.split(',').next().map(|ip| ip.trim().to_string());
        }
    }
    
    // X-Real-IP
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            return Some(ip_str.to_string());
        }
    }
    
    // CF-Connecting-IP (Cloudflare)
    if let Some(cf_ip) = headers.get("cf-connecting-ip") {
        if let Ok(ip_str) = cf_ip.to_str() {
            return Some(ip_str.to_string());
        }
    }
    
    None
}

fn extract_required_feature(request: &Request) -> Option<String> {
    // Check for feature requirement in headers
    if let Some(feature_header) = request.headers().get("x-required-feature") {
        if let Ok(feature) = feature_header.to_str() {
            return Some(feature.to_string());
        }
    }
    
    // Extract feature from path patterns
    let path = request.uri().path();
    
    // Map certain paths to features
    if path.contains("/analytics") {
        return Some("analytics".to_string());
    }
    
    if path.contains("/export") {
        return Some("data_export".to_string());
    }
    
    if path.contains("/admin") {
        return Some("admin_panel".to_string());
    }
    
    None
}