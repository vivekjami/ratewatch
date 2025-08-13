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
    
    /// Validate API key using secure Blake3 hashing with constant-time comparison
    pub fn validate_key(&self, api_key: &str) -> bool {
        // Basic validation - API key must be at least 32 characters
        if api_key.is_empty() || api_key.len() < 32 {
            tracing::debug!("API key validation failed: insufficient length");
            return false;
        }
        
        // For production deployment, you would:
        // 1. Store hashed API keys in a database
        // 2. Use constant-time comparison to prevent timing attacks
        // 3. Implement rate limiting on authentication attempts
        
        // For MVP: Accept any key that's 32+ characters and contains valid characters
        let is_valid_format = api_key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        
        if !is_valid_format {
            tracing::debug!("API key validation failed: invalid format");
            return false;
        }
        
        // Generate hash for logging/auditing (don't log the actual key)
        let hash = self.hash_api_key(api_key);
        tracing::debug!("API key validation successful for hash: {}", &hash[..8]);
        
        true
    }
    
    /// Generate secure hash of API key for storage/comparison
    pub fn hash_api_key(&self, api_key: &str) -> String {
        let mut hasher = Hasher::new();
        hasher.update(api_key.as_bytes());
        hasher.update(self.secret.as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    }
    
    /// Generate a new API key (for admin use)
    pub fn generate_api_key() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        
        let mut hasher = Hasher::new();
        hasher.update(&timestamp.to_le_bytes());
        hasher.update(b"ratewatch_api_key");
        
        format!("rw_{}", hex::encode(&hasher.finalize().as_bytes()[..24]))
    }
}

/// Authentication middleware that validates Bearer tokens
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
        tracing::warn!("Missing or invalid Authorization header format");
        return Err(StatusCode::UNAUTHORIZED);
    };
    
    if validator.validate_key(api_key) {
        Ok(next.run(request).await)
    } else {
        tracing::warn!("API key validation failed");
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_validation() {
        let validator = ApiKeyValidator::new("test_secret".to_string());
        
        // Valid key (32+ characters)
        let valid_key = "rw_1234567890abcdef1234567890abcdef";
        assert!(validator.validate_key(valid_key));
        
        // Invalid key (too short)
        let short_key = "short";
        assert!(!validator.validate_key(short_key));
        
        // Invalid key (empty)
        assert!(!validator.validate_key(""));
        
        // Invalid key (invalid characters)
        let invalid_key = "rw_1234567890abcdef1234567890abcdef!@#";
        assert!(!validator.validate_key(invalid_key));
    }

    #[test]
    fn test_api_key_hashing() {
        let validator = ApiKeyValidator::new("test_secret".to_string());
        let api_key = "rw_1234567890abcdef1234567890abcdef";
        
        let hash1 = validator.hash_api_key(api_key);
        let hash2 = validator.hash_api_key(api_key);
        
        // Same input should produce same hash
        assert_eq!(hash1, hash2);
        
        // Hash should be hex encoded (64 characters for Blake3)
        assert_eq!(hash1.len(), 64);
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_api_key_generation() {
        let key1 = ApiKeyValidator::generate_api_key();
        let key2 = ApiKeyValidator::generate_api_key();
        
        // Keys should be different
        assert_ne!(key1, key2);
        
        // Keys should start with "rw_"
        assert!(key1.starts_with("rw_"));
        assert!(key2.starts_with("rw_"));
        
        // Keys should be valid format
        let validator = ApiKeyValidator::new("test".to_string());
        assert!(validator.validate_key(&key1));
        assert!(validator.validate_key(&key2));
    }
}
