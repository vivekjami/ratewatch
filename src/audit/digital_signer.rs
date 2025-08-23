use anyhow::Result;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct DigitalSigner {
    key: Vec<u8>,
}

impl DigitalSigner {
    pub fn new(key: &str) -> Result<Self> {
        if key.len() < 32 {
            return Err(anyhow::anyhow!(
                "Signing key must be at least 32 characters long for security"
            ));
        }
        
        Ok(Self {
            key: key.as_bytes().to_vec(),
        })
    }

    /// Sign a message and return the signature as a hex string
    pub fn sign(&self, message: &str) -> Result<String> {
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|e| anyhow::anyhow!("Failed to create HMAC: {}", e))?;
        
        mac.update(message.as_bytes());
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());
        
        Ok(signature)
    }

    /// Verify a signature against a message
    pub fn verify(&self, message: &str, signature: &str) -> Result<bool> {
        let expected_signature = self.sign(message)?;
        Ok(constant_time_eq::constant_time_eq(
            signature.as_bytes(),
            expected_signature.as_bytes(),
        ))
    }

    /// Generate a new random signing key (for key rotation)
    pub fn generate_key() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let key: Vec<u8> = (0..64).map(|_| rng.gen()).collect();
        hex::encode(key)
    }
}

impl fmt::Debug for DigitalSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DigitalSigner")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify() {
        let key = "this-is-a-test-key-that-is-long-enough-for-security";
        let signer = DigitalSigner::new(key).unwrap();
        
        let message = "test message";
        let signature = signer.sign(message).unwrap();
        
        assert!(signer.verify(message, &signature).unwrap());
        assert!(!signer.verify("different message", &signature).unwrap());
    }

    #[test]
    fn test_key_too_short() {
        let result = DigitalSigner::new("short");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_key() {
        let key = DigitalSigner::generate_key();
        assert!(key.len() >= 64); // 32 bytes = 64 hex chars
        
        // Should be able to create a signer with generated key
        let signer = DigitalSigner::new(&key);
        assert!(signer.is_ok());
    }
}