use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use blake3::Hasher;
use std::sync::Arc;

pub struct ApiKeyValidator {
    secret: String,
}

impl ApiKeyValidator {
    pub fn new(secret: String) -> Self {
        Self { secret }
    }
    
    pub fn validate_key(&self, api_key: &str) -> bool {
        // Basic validation - API key must be at least 32 characters
        if api_key.is_empty() || api_key.len() < 32 {
            return false;
        }
        
        // For MVP: Simple validation
        // In production, you would check against a database of hashed API keys
        let mut hasher = Hasher::new();
        hasher.update(api_key.as_bytes());
        hasher.update(self.secret.as_bytes());
        let _hash = hasher.finalize();
        
        // For demo purposes, accept any key that's 32+ characters
        // In production, replace with proper database lookup
        true
    }
    
    pub fn hash_api_key(&self, api_key: &str) -> String {
        let mut hasher = Hasher::new();
        hasher.update(api_key.as_bytes());
        hasher.update(self.secret.as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    }
}

pub async fn auth_middleware(
    State(validator): State<Arc<ApiKeyValidator>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let api_key = if auth_header.starts_with("Bearer ") {
        &auth_header[7..]
    } else {
        tracing::warn!("Missing or invalid Authorization header");
        return Err(StatusCode::UNAUTHORIZED);
    };
    
    if validator.validate_key(api_key) {
        tracing::debug!("API key validation successful");
        Ok(next.run(request).await)
    } else {
        tracing::warn!("API key validation failed");
        Err(StatusCode::UNAUTHORIZED)
    }
}
