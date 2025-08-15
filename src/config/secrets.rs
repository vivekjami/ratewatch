use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::env;

/// Secret manager with pluggable providers
pub struct SecretManager {
    providers: HashMap<String, Box<dyn SecretProvider>>,
    default_provider: String,
}

impl SecretManager {
    pub async fn new() -> Result<Self> {
        let mut providers: HashMap<String, Box<dyn SecretProvider>> = HashMap::new();
        
        // Always include environment provider
        providers.insert("env".to_string(), Box::new(EnvSecretProvider::new()));

        // Add Vault provider if configured
        if env::var("VAULT_ADDR").is_ok() {
            match VaultSecretProvider::new().await {
                Ok(provider) => {
                    providers.insert("vault".to_string(), Box::new(provider));
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize Vault secret provider: {}", e);
                }
            }
        }

        // Add AWS Secrets Manager if configured
        if env::var("AWS_REGION").is_ok() || env::var("AWS_DEFAULT_REGION").is_ok() {
            match AwsSecretsProvider::new().await {
                Ok(provider) => {
                    providers.insert("aws".to_string(), Box::new(provider));
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize AWS Secrets Manager provider: {}", e);
                }
            }
        }

        // Add Azure Key Vault if configured
        if env::var("AZURE_KEYVAULT_URL").is_ok() {
            match AzureKeyVaultProvider::new().await {
                Ok(provider) => {
                    providers.insert("azure".to_string(), Box::new(provider));
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize Azure Key Vault provider: {}", e);
                }
            }
        }

        let default_provider = env::var("SECRET_PROVIDER").unwrap_or_else(|_| "env".to_string());

        if !providers.contains_key(&default_provider) {
            return Err(anyhow::anyhow!(
                "Default secret provider '{}' is not available", default_provider
            ));
        }

        tracing::info!("Initialized secret manager with {} providers", providers.len());
        
        Ok(Self {
            providers,
            default_provider,
        })
    }

    pub async fn get_secret(&self, key: &str) -> Result<String> {
        // Parse provider from key if specified (e.g., "vault:my-secret")
        let (provider_name, secret_key) = if key.contains(':') {
            let parts: Vec<&str> = key.splitn(2, ':').collect();
            (parts[0], parts[1])
        } else {
            (self.default_provider.as_str(), key)
        };

        let provider = self.providers.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Secret provider '{}' not found", provider_name))?;

        provider.get_secret(secret_key).await
            .with_context(|| format!("Failed to get secret '{}' from provider '{}'", secret_key, provider_name))
    }

    pub async fn rotate_secret(&self, key: &str) -> Result<()> {
        let (provider_name, secret_key) = if key.contains(':') {
            let parts: Vec<&str> = key.splitn(2, ':').collect();
            (parts[0], parts[1])
        } else {
            (self.default_provider.as_str(), key)
        };

        let provider = self.providers.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Secret provider '{}' not found", provider_name))?;

        provider.rotate_secret(secret_key).await
            .with_context(|| format!("Failed to rotate secret '{}' in provider '{}'", secret_key, provider_name))
    }

    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

/// Secret provider trait
#[async_trait]
pub trait SecretProvider: Send + Sync {
    async fn get_secret(&self, key: &str) -> Result<String>;
    async fn rotate_secret(&self, key: &str) -> Result<()>;
    fn name(&self) -> &str;
}

/// Environment variable secret provider
pub struct EnvSecretProvider;

impl EnvSecretProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SecretProvider for EnvSecretProvider {
    async fn get_secret(&self, key: &str) -> Result<String> {
        env::var(key)
            .with_context(|| format!("Environment variable '{}' not found", key))
    }

    async fn rotate_secret(&self, _key: &str) -> Result<()> {
        Err(anyhow::anyhow!("Secret rotation not supported for environment variables"))
    }

    fn name(&self) -> &str {
        "env"
    }
}

/// HashiCorp Vault secret provider
pub struct VaultSecretProvider {
    client: Option<vault::Client>,
    mount_path: String,
}

impl VaultSecretProvider {
    pub async fn new() -> Result<Self> {
        let vault_addr = env::var("VAULT_ADDR")
            .context("VAULT_ADDR environment variable required")?;

        let client = vault::Client::new(&vault_addr)
            .context("Failed to create Vault client")?;

        // Authenticate with Vault
        if let Ok(token) = env::var("VAULT_TOKEN") {
            client.set_token(&token);
        } else if let (Ok(role_id), Ok(secret_id)) = (
            env::var("VAULT_ROLE_ID"),
            env::var("VAULT_SECRET_ID")
        ) {
            let auth_info = vault::auth::approle::login(&client, &role_id, &secret_id).await
                .context("Failed to authenticate with Vault using AppRole")?;
            client.set_token(&auth_info.client_token);
        } else {
            return Err(anyhow::anyhow!(
                "No Vault authentication method configured"
            ));
        }

        let mount_path = env::var("VAULT_SECRETS_MOUNT")
            .unwrap_or_else(|_| "secret".to_string());

        Ok(Self {
            client: Some(client),
            mount_path,
        })
    }
}

#[async_trait]
impl SecretProvider for VaultSecretProvider {
    async fn get_secret(&self, key: &str) -> Result<String> {
        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Vault client not initialized"))?;

        let secret = vault::kv2::read(client, &self.mount_path, key).await
            .with_context(|| format!("Failed to read secret '{}' from Vault", key))?;

        // Extract the secret value (assuming it's stored under a "value" key)
        if let Some(data) = secret.data {
            if let Some(value) = data.get("value") {
                Ok(value.clone())
            } else if let Some((_, value)) = data.iter().next() {
                // If no "value" key, use the first key-value pair
                Ok(value.clone())
            } else {
                Err(anyhow::anyhow!("Secret '{}' has no data", key))
            }
        } else {
            Err(anyhow::anyhow!("Secret '{}' not found", key))
        }
    }

    async fn rotate_secret(&self, key: &str) -> Result<()> {
        // Vault secret rotation would typically involve:
        // 1. Generate new secret value
        // 2. Update the secret in Vault
        // 3. Notify dependent systems
        
        tracing::info!("Secret rotation for '{}' requested (not implemented)", key);
        Err(anyhow::anyhow!("Vault secret rotation not yet implemented"))
    }

    fn name(&self) -> &str {
        "vault"
    }
}

/// AWS Secrets Manager provider
pub struct AwsSecretsProvider {
    client: Option<aws_sdk_secretsmanager::Client>,
}

impl AwsSecretsProvider {
    pub async fn new() -> Result<Self> {
        let config = aws_config::load_from_env().await;
        let client = aws_sdk_secretsmanager::Client::new(&config);

        Ok(Self {
            client: Some(client),
        })
    }
}

#[async_trait]
impl SecretProvider for AwsSecretsProvider {
    async fn get_secret(&self, key: &str) -> Result<String> {
        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("AWS Secrets Manager client not initialized"))?;

        let response = client
            .get_secret_value()
            .secret_id(key)
            .send()
            .await
            .with_context(|| format!("Failed to get secret '{}' from AWS Secrets Manager", key))?;

        response.secret_string()
            .ok_or_else(|| anyhow::anyhow!("Secret '{}' has no string value", key))
            .map(|s| s.to_string())
    }

    async fn rotate_secret(&self, key: &str) -> Result<()> {
        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("AWS Secrets Manager client not initialized"))?;

        client
            .rotate_secret()
            .secret_id(key)
            .send()
            .await
            .with_context(|| format!("Failed to rotate secret '{}' in AWS Secrets Manager", key))?;

        tracing::info!("Successfully initiated rotation for secret '{}'", key);
        Ok(())
    }

    fn name(&self) -> &str {
        "aws"
    }
}

/// Azure Key Vault provider
pub struct AzureKeyVaultProvider {
    vault_url: String,
    client: Option<azure_security_keyvault::KeyVaultClient>,
}

impl AzureKeyVaultProvider {
    pub async fn new() -> Result<Self> {
        let vault_url = env::var("AZURE_KEYVAULT_URL")
            .context("AZURE_KEYVAULT_URL environment variable required")?;

        // Azure authentication would be set up here
        // For now, we'll create a placeholder
        
        Ok(Self {
            vault_url,
            client: None, // Would initialize actual client here
        })
    }
}

#[async_trait]
impl SecretProvider for AzureKeyVaultProvider {
    async fn get_secret(&self, key: &str) -> Result<String> {
        // Azure Key Vault implementation would go here
        tracing::warn!("Azure Key Vault provider not fully implemented");
        Err(anyhow::anyhow!("Azure Key Vault provider not implemented"))
    }

    async fn rotate_secret(&self, _key: &str) -> Result<()> {
        Err(anyhow::anyhow!("Azure Key Vault secret rotation not implemented"))
    }

    fn name(&self) -> &str {
        "azure"
    }
}

// Mock modules for compilation when features are not enabled
#[cfg(not(feature = "vault"))]
mod vault {
    use anyhow::Result;
    use std::collections::HashMap;

    pub struct Client;
    
    impl Client {
        pub fn new(_addr: &str) -> Result<Self> {
            Err(anyhow::anyhow!("Vault support not compiled in"))
        }
        
        pub fn set_token(&self, _token: &str) {}
    }

    pub mod auth {
        pub mod approle {
            use super::super::*;
            
            pub struct AuthInfo {
                pub client_token: String,
            }
            
            pub async fn login(_client: &Client, _role_id: &str, _secret_id: &str) -> Result<AuthInfo> {
                Err(anyhow::anyhow!("Vault support not compiled in"))
            }
        }
    }

    pub mod kv2 {
        use super::*;
        
        pub struct Secret {
            pub data: Option<HashMap<String, String>>,
        }
        
        pub async fn read(_client: &Client, _mount: &str, _path: &str) -> Result<Secret> {
            Err(anyhow::anyhow!("Vault support not compiled in"))
        }
    }
}

#[cfg(not(feature = "aws"))]
mod aws_config {
    use anyhow::Result;
    
    pub struct SdkConfig;
    
    pub async fn load_from_env() -> SdkConfig {
        SdkConfig
    }
}

#[cfg(not(feature = "aws"))]
mod aws_sdk_secretsmanager {
    use super::aws_config::SdkConfig;
    use anyhow::Result;
    
    pub struct Client;
    
    impl Client {
        pub fn new(_config: &SdkConfig) -> Self {
            Self
        }
        
        pub fn get_secret_value(&self) -> GetSecretValueFluentBuilder {
            GetSecretValueFluentBuilder
        }
        
        pub fn rotate_secret(&self) -> RotateSecretFluentBuilder {
            RotateSecretFluentBuilder
        }
    }
    
    pub struct GetSecretValueFluentBuilder;
    
    impl GetSecretValueFluentBuilder {
        pub fn secret_id(self, _id: &str) -> Self {
            self
        }
        
        pub async fn send(self) -> Result<GetSecretValueOutput> {
            Err(anyhow::anyhow!("AWS SDK not compiled in"))
        }
    }
    
    pub struct RotateSecretFluentBuilder;
    
    impl RotateSecretFluentBuilder {
        pub fn secret_id(self, _id: &str) -> Self {
            self
        }
        
        pub async fn send(self) -> Result<RotateSecretOutput> {
            Err(anyhow::anyhow!("AWS SDK not compiled in"))
        }
    }
    
    pub struct GetSecretValueOutput;
    
    impl GetSecretValueOutput {
        pub fn secret_string(&self) -> Option<&str> {
            None
        }
    }
    
    pub struct RotateSecretOutput;
}

#[cfg(not(feature = "azure"))]
mod azure_security_keyvault {
    pub struct KeyVaultClient;
}