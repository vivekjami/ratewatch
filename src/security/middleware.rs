use crate::security::{ThreatDetector, threat_analyzer::RequestContext};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Middleware to perform threat detection on incoming requests
pub async fn threat_detection_middleware(
    State(threat_detector): State<Arc<ThreatDetector>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract request information for threat analysis
    let ip_address = extract_ip_address(&request).unwrap_or_else(|| "unknown".to_string());
    let user_agent = extract_user_agent(&request);
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    
    // Create request context for threat analysis
    let mut context = RequestContext::new(ip_address.clone(), path.clone(), method.clone());
    
    if let Some(ua) = user_agent {
        context = context.with_user_agent(ua);
    }
    
    // Add headers to context
    for (name, value) in request.headers().iter() {
        if let Ok(value_str) = value.to_str() {
            context = context.with_header(name.to_string(), value_str.to_string());
        }
    }
    
    // Perform threat analysis
    match threat_detector.analyze_request(&context).await {
        Ok(analysis_result) => {
            debug!(
                ip_address = ip_address,
                threat_score = analysis_result.overall_score.score,
                confidence = analysis_result.overall_score.confidence,
                actions_taken = analysis_result.actions_taken.len(),
                "Threat analysis completed"
            );
            
            // Check if the request should be blocked
            if analysis_result.requires_action() && !analysis_result.actions_taken.is_empty() {
                warn!(
                    ip_address = ip_address,
                    threat_score = analysis_result.overall_score.score,
                    reasons = ?analysis_result.overall_score.reasons,
                    "Request blocked due to threat detection"
                );
                
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            
            // Add analysis result to request extensions for downstream use
            request.extensions_mut().insert(analysis_result);
        }
        Err(e) => {
            error!(
                ip_address = ip_address,
                error = %e,
                "Threat analysis failed"
            );
            // Continue processing even if threat analysis fails
        }
    }
    
    // Continue with the request
    let response = next.run(request).await;
    Ok(response)
}

fn extract_ip_address(request: &Request) -> Option<String> {
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
    
    None
}

fn extract_user_agent(request: &Request) -> Option<String> {
    request
        .headers()
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .map(|s| s.to_string())
}

/// Extract threat analysis result from request extensions
pub fn get_threat_analysis_result(request: &Request) -> Option<&crate::security::threat_detector::ThreatAnalysisResult> {
    request.extensions().get::<crate::security::threat_detector::ThreatAnalysisResult>()
}