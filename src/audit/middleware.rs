use crate::audit::{AuditLogger, audit_event::ActorInfo};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use uuid::Uuid;

/// Middleware to automatically log API requests to the audit system
pub async fn audit_middleware(
    State(audit_logger): State<Arc<AuditLogger>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let correlation_id = Uuid::new_v4();
    
    // Extract request information
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let ip_address = extract_ip_address(&request);
    let user_agent = extract_user_agent(&request);
    let api_key_id = extract_api_key_id(&request);
    
    // Add correlation ID to request extensions for downstream use
    request.extensions_mut().insert(correlation_id);
    
    // Process the request
    let response = next.run(request).await;
    let status_code = response.status().as_u16();
    
    // Create actor info
    let actor = ActorInfo::new()
        .with_ip_address(ip_address.unwrap_or_else(|| "unknown".to_string()))
        .with_user_agent(user_agent.unwrap_or_else(|| "unknown".to_string()));
    
    let actor = if let Some(key_id) = api_key_id {
        actor.with_api_key(key_id)
    } else {
        actor
    };
    
    // Log the API request asynchronously (don't block response)
    let audit_logger_clone = audit_logger.clone();
    tokio::spawn(async move {
        if let Err(e) = audit_logger_clone
            .log_api_request(
                actor,
                &method,
                &path,
                status_code,
                None, // tenant_id - would be extracted from request context
                Some(correlation_id),
            )
            .await
        {
            tracing::error!("Failed to log audit event: {}", e);
        }
    });
    
    Ok(response)
}

fn extract_ip_address(request: &Request) -> Option<String> {
    // Try various headers for IP address
    let headers = request.headers();
    
    // Check X-Forwarded-For first (most common for proxied requests)
    if let Some(forwarded_for) = headers.get("x-forwarded-for") {
        if let Ok(value) = forwarded_for.to_str() {
            // Take the first IP in the chain
            if let Some(first_ip) = value.split(',').next() {
                return Some(first_ip.trim().to_string());
            }
        }
    }
    
    // Check X-Real-IP
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            return Some(value.to_string());
        }
    }
    
    // Check CF-Connecting-IP (Cloudflare)
    if let Some(cf_ip) = headers.get("cf-connecting-ip") {
        if let Ok(value) = cf_ip.to_str() {
            return Some(value.to_string());
        }
    }
    
    // Fallback to connection info (not available in this context)
    None
}

fn extract_user_agent(request: &Request) -> Option<String> {
    request
        .headers()
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
}

fn extract_api_key_id(request: &Request) -> Option<String> {
    // This would typically extract the API key ID from the Authorization header
    // or from request extensions if it was set by the auth middleware
    request
        .extensions()
        .get::<String>() // Assuming auth middleware stores the API key ID
        .cloned()
}

/// Extract correlation ID from request extensions
pub fn get_correlation_id(request: &Request) -> Option<Uuid> {
    request.extensions().get::<Uuid>().copied()
}