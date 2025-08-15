use super::{ConfigMap, ConfigSource, ConfigChange, ConfigChangeType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::env;
use std::path::Path;
use tokio::fs;
use tokio::sync::mpsc;

/// Environment variable configuration source
pub struct EnvConfigSource {
    prefix: String,
}

impl EnvConfigSource {
    pub fn new() -> Self {
        Self {
            prefix: "RATEWATCH_".to_string(),
        }
    }

    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

#[async_trait]
impl ConfigSource for EnvConfigSource {
    async fn load_config(&self) -> Result<ConfigMap> {
        let mut config = ConfigMap::new();

        for (key, value) in env::vars() {
            if key.starts_with(&self.prefix) {
                let config_key = key
                    .strip_prefix(&self.prefix)
                    .unwrap()
                    .to_lowercase()
                    .replace('_', ".");
                
                // Try to parse as JSON first, then fall back to string
                let parsed_value = serde_json::from_str(&value)
                    .unwrap_or_else(|_| serde_json::Value::String(value));
                
                config.insert(config_key, parsed_value);
            }
        }

        // Also load standard environment variables
        if let Ok(port) = env::var("PORT") {
            config.insert("server.port".to_string(), 
                serde_json::Value::String(port));
        }

        if let Ok(redis_url) = env::var("REDIS_URL") {
            config.insert("redis.url".to_string(), 
                serde_json::Value::String(redis_url));
        }

        if let Ok(log_level) = env::var("RUST_LOG") {
            config.insert("observability.logging.level".to_string(), 
                serde_json::Value::String(log_level));
        }

        if let Ok(api_key_secret) = env::var("API_KEY_SECRET") {
            config.insert("security.api_key_secret".to_string(), 
                serde_json::Value::String(api_key_secret));
        }

        tracing::debug!("Loaded {} configuration values from environment", config.len());
        Ok(config)
    }

    async fn watch_changes(&self) -> Result<mpsc::Receiver<ConfigChange>> {
        let (tx, rx) = mpsc::channel(100);
        
        // Environment variables don't typically change during runtime,
        // but we can still provide a channel for consistency
        tracing::debug!("Environment config source doesn't support change watching");
        
        Ok(rx)
    }

    fn name(&self) -> &str {
        "environment"
    }
}

/// File-based configuration source
pub struct FileConfigSource {
    file_path: String,
}

impl FileConfigSource {
    pub fn new(file_path: impl Into<String>) -> Result<Self> {
        let file_path = file_path.into();
        Ok(Self { file_path })
    }
}

#[async_trait]
impl ConfigSource for FileConfigSource {
    async fn load_config(&self) -> Result<ConfigMap> {
        if !Path::new(&self.file_path).exists() {
            tracing::debug!("Configuration file {} does not exist", self.file_path);
            return Ok(ConfigMap::new());
        }

        let content = fs::read_to_string(&self.file_path).await
            .with_context(|| format!("Failed to read config file: {}", self.file_path))?;

        let config: ConfigMap = if self.file_path.ends_with(".toml") {
            let toml_value: toml::Value = toml::from_str(&content)
                .with_context(|| format!("Failed to parse TOML config: {}", self.file_path))?;
            self.flatten_toml_value("", &toml_value)
        } else if self.file_path.ends_with(".yaml") || self.file_path.ends_with(".yml") {
            let yaml_value: serde_yaml::Value = serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse YAML config: {}", self.file_path))?;
            self.flatten_yaml_value("", &yaml_value)
        } else if self.file_path.ends_with(".json") {
            let json_value: serde_json::Value = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse JSON config: {}", self.file_path))?;
            self.flatten_json_value("", &json_value)
        } else {
            return Err(anyhow::anyhow!("Unsupported config file format: {}", self.file_path));
        };

        tracing::debug!("Loaded {} configuration values from file: {}", 
            config.len(), self.file_path);
        Ok(config)
    }

    async fn watch_changes(&self) -> Result<mpsc::Receiver<ConfigChange>> {
        let (tx, rx) = mpsc::channel(100);
        let file_path = self.file_path.clone();

        tokio::spawn(async move {
            let (watch_tx, mut watch_rx) = mpsc::channel(100);
            
            let mut watcher = match RecommendedWatcher::new(
                move |res: notify::Result<Event>| {
                    if let Ok(event) = res {
                        let _ = watch_tx.try_send(event);
                    }
                },
                notify::Config::default(),
            ) {
                Ok(watcher) => watcher,
                Err(e) => {
                    tracing::error!("Failed to create file watcher: {}", e);
                    return;
                }
            };

            if let Err(e) = watcher.watch(Path::new(&file_path), RecursiveMode::NonRecursive) {
                tracing::error!("Failed to watch config file {}: {}", file_path, e);
                return;
            }

            while let Some(event) = watch_rx.recv().await {
                if matches!(event.kind, EventKind::Modify(_)) {
                    let change = ConfigChange {
                        source: "file".to_string(),
                        change_type: ConfigChangeType::Modified,
                        affected_keys: vec![], // We don't know specific keys from file events
                    };
                    
                    if tx.send(change).await.is_err() {
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    fn name(&self) -> &str {
        "file"
    }
}

impl FileConfigSource {
    fn flatten_toml_value(&self, prefix: &str, value: &toml::Value) -> ConfigMap {
        let mut config = ConfigMap::new();
        
        match value {
            toml::Value::Table(table) => {
                for (key, val) in table {
                    let new_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    config.extend(self.flatten_toml_value(&new_prefix, val));
                }
            }
            _ => {
                let json_value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
                config.insert(prefix.to_string(), json_value);
            }
        }
        
        config
    }

    fn flatten_yaml_value(&self, prefix: &str, value: &serde_yaml::Value) -> ConfigMap {
        let mut config = ConfigMap::new();
        
        match value {
            serde_yaml::Value::Mapping(mapping) => {
                for (key, val) in mapping {
                    if let Some(key_str) = key.as_str() {
                        let new_prefix = if prefix.is_empty() {
                            key_str.to_string()
                        } else {
                            format!("{}.{}", prefix, key_str)
                        };
                        config.extend(self.flatten_yaml_value(&new_prefix, val));
                    }
                }
            }
            _ => {
                let json_value = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
                config.insert(prefix.to_string(), json_value);
            }
        }
        
        config
    }

    fn flatten_json_value(&self, prefix: &str, value: &serde_json::Value) -> ConfigMap {
        let mut config = ConfigMap::new();
        
        match value {
            serde_json::Value::Object(object) => {
                for (key, val) in object {
                    let new_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    config.extend(self.flatten_json_value(&new_prefix, val));
                }
            }
            _ => {
                config.insert(prefix.to_string(), value.clone());
            }
        }
        
        config
    }
}

/// HashiCorp Vault configuration source
pub struct VaultConfigSource {
    client: Option<vault::Client>,
    mount_path: String,
    secret_path: String,
}

impl VaultConfigSource {
    pub async fn new() -> Result<Self> {
        let vault_addr = env::var("VAULT_ADDR")
            .context("VAULT_ADDR environment variable required for Vault config source")?;
        
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
                "No Vault authentication method configured. Set VAULT_TOKEN or VAULT_ROLE_ID/VAULT_SECRET_ID"
            ));
        }

        Ok(Self {
            client: Some(client),
            mount_path: env::var("VAULT_MOUNT_PATH").unwrap_or_else(|_| "secret".to_string()),
            secret_path: env::var("VAULT_SECRET_PATH").unwrap_or_else(|_| "ratewatch/config".to_string()),
        })
    }
}

#[async_trait]
impl ConfigSource for VaultConfigSource {
    async fn load_config(&self) -> Result<ConfigMap> {
        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Vault client not initialized"))?;

        let secret = vault::kv2::read(client, &self.mount_path, &self.secret_path).await
            .context("Failed to read configuration from Vault")?;

        let mut config = ConfigMap::new();
        if let Some(data) = secret.data {
            for (key, value) in data {
                let json_value = serde_json::to_value(&value)
                    .unwrap_or(serde_json::Value::String(value));
                config.insert(key, json_value);
            }
        }

        tracing::debug!("Loaded {} configuration values from Vault", config.len());
        Ok(config)
    }

    async fn watch_changes(&self) -> Result<mpsc::Receiver<ConfigChange>> {
        let (tx, rx) = mpsc::channel(100);
        
        // Vault doesn't have built-in change notifications, but we could implement polling
        tracing::debug!("Vault config source doesn't support real-time change watching");
        
        Ok(rx)
    }

    fn name(&self) -> &str {
        "vault"
    }
}

/// Kubernetes ConfigMap/Secret configuration source
pub struct K8sConfigSource {
    namespace: String,
    configmap_name: Option<String>,
    secret_name: Option<String>,
}

impl K8sConfigSource {
    pub async fn new() -> Result<Self> {
        let namespace = env::var("K8S_NAMESPACE")
            .unwrap_or_else(|_| "default".to_string());
        
        let configmap_name = env::var("K8S_CONFIGMAP_NAME").ok();
        let secret_name = env::var("K8S_SECRET_NAME").ok();

        if configmap_name.is_none() && secret_name.is_none() {
            return Err(anyhow::anyhow!(
                "At least one of K8S_CONFIGMAP_NAME or K8S_SECRET_NAME must be set"
            ));
        }

        Ok(Self {
            namespace,
            configmap_name,
            secret_name,
        })
    }
}

#[async_trait]
impl ConfigSource for K8sConfigSource {
    async fn load_config(&self) -> Result<ConfigMap> {
        let mut config = ConfigMap::new();

        // Load from ConfigMap if specified
        if let Some(configmap_name) = &self.configmap_name {
            match self.load_from_configmap(configmap_name).await {
                Ok(cm_config) => config.extend(cm_config),
                Err(e) => tracing::warn!("Failed to load from ConfigMap {}: {}", configmap_name, e),
            }
        }

        // Load from Secret if specified
        if let Some(secret_name) = &self.secret_name {
            match self.load_from_secret(secret_name).await {
                Ok(secret_config) => config.extend(secret_config),
                Err(e) => tracing::warn!("Failed to load from Secret {}: {}", secret_name, e),
            }
        }

        tracing::debug!("Loaded {} configuration values from Kubernetes", config.len());
        Ok(config)
    }

    async fn watch_changes(&self) -> Result<mpsc::Receiver<ConfigChange>> {
        let (tx, rx) = mpsc::channel(100);
        
        // Kubernetes watch implementation would go here
        // For now, we'll just return an empty channel
        tracing::debug!("Kubernetes config source change watching not yet implemented");
        
        Ok(rx)
    }

    fn name(&self) -> &str {
        "kubernetes"
    }
}

impl K8sConfigSource {
    async fn load_from_configmap(&self, name: &str) -> Result<ConfigMap> {
        // This would use the Kubernetes API to load ConfigMap data
        // For now, we'll simulate by reading from mounted volumes
        let configmap_path = format!("/etc/config/{}", name);
        
        if !Path::new(&configmap_path).exists() {
            return Ok(ConfigMap::new());
        }

        let mut config = ConfigMap::new();
        let mut entries = fs::read_dir(&configmap_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let key = entry.file_name().to_string_lossy().to_string();
                let content = fs::read_to_string(entry.path()).await?;
                
                // Try to parse as JSON, fall back to string
                let value = serde_json::from_str(&content)
                    .unwrap_or_else(|_| serde_json::Value::String(content));
                
                config.insert(key, value);
            }
        }

        Ok(config)
    }

    async fn load_from_secret(&self, name: &str) -> Result<ConfigMap> {
        // This would use the Kubernetes API to load Secret data
        // For now, we'll simulate by reading from mounted volumes
        let secret_path = format!("/etc/secrets/{}", name);
        
        if !Path::new(&secret_path).exists() {
            return Ok(ConfigMap::new());
        }

        let mut config = ConfigMap::new();
        let mut entries = fs::read_dir(&secret_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let key = entry.file_name().to_string_lossy().to_string();
                let content = fs::read_to_string(entry.path()).await?;
                
                // Secrets are typically base64 encoded, but mounted secrets are decoded
                let value = serde_json::Value::String(content);
                config.insert(key, value);
            }
        }

        Ok(config)
    }
}

// Mock Vault module for compilation (would be replaced with actual vault crate)
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