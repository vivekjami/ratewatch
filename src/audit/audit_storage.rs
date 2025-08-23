use crate::audit::audit_event::AuditEvent;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde_json;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use tokio::fs;
use uuid::Uuid;

#[async_trait]
pub trait AuditStorage: Send + Sync {
    async fn store_event(&self, event: &AuditEvent) -> Result<()>;
    async fn get_event(&self, event_id: &Uuid) -> Result<Option<AuditEvent>>;
    async fn get_events_by_timerange(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<AuditEvent>>;
    async fn get_events_by_actor(
        &self,
        actor_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<Vec<AuditEvent>>;
    async fn verify_integrity(&self) -> Result<bool>;
}

pub struct RedisAuditStorage {
    client: redis::Client,
}

impl RedisAuditStorage {
    pub fn new(client: redis::Client) -> Self {
        Self { client }
    }

    fn event_key(&self, event_id: &Uuid) -> String {
        format!("audit:event:{}", event_id)
    }

    fn tenant_index_key(&self, tenant_id: &str, date: &str) -> String {
        format!("audit:tenant:{}:date:{}", tenant_id, date)
    }

    fn actor_index_key(&self, actor_id: &str, tenant_id: Option<&str>) -> String {
        match tenant_id {
            Some(tid) => format!("audit:tenant:{}:actor:{}", tid, actor_id),
            None => format!("audit:actor:{}", actor_id),
        }
    }

    fn global_index_key(&self, date: &str) -> String {
        format!("audit:global:date:{}", date)
    }
}

#[async_trait]
impl AuditStorage for RedisAuditStorage {
    async fn store_event(&self, event: &AuditEvent) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        
        let event_json = serde_json::to_string(event)?;
        let event_key = self.event_key(&event.id);
        let date_str = event.timestamp.format("%Y-%m-%d").to_string();
        
        // Store the event
        conn.set::<_, _, ()>(&event_key, &event_json).await?;
        
        // Add to date-based indices
        let global_index = self.global_index_key(&date_str);
        conn.zadd::<_, _, _, ()>(&global_index, event.timestamp.timestamp(), &event.id.to_string()).await?;
        
        // Add to tenant-specific index if tenant_id exists
        if let Some(tenant_id) = &event.tenant_id {
            let tenant_index = self.tenant_index_key(tenant_id, &date_str);
            conn.zadd::<_, _, _, ()>(&tenant_index, event.timestamp.timestamp(), &event.id.to_string()).await?;
        }
        
        // Add to actor-specific index
        if let Some(actor_id) = &event.actor.user_id {
            let actor_index = self.actor_index_key(actor_id, event.tenant_id.as_deref());
            conn.zadd::<_, _, _, ()>(&actor_index, event.timestamp.timestamp(), &event.id.to_string()).await?;
        }
        
        if let Some(api_key_id) = &event.actor.api_key_id {
            let actor_index = self.actor_index_key(api_key_id, event.tenant_id.as_deref());
            conn.zadd::<_, _, _, ()>(&actor_index, event.timestamp.timestamp(), &event.id.to_string()).await?;
        }
        
        // Set expiration for the event (default 7 years for compliance)
        let expiration_seconds = 7 * 365 * 24 * 60 * 60; // 7 years
        conn.expire::<_, ()>(&event_key, expiration_seconds).await?;
        
        Ok(())
    }

    async fn get_event(&self, event_id: &Uuid) -> Result<Option<AuditEvent>> {
        let mut conn = self.client.get_async_connection().await?;
        let event_key = self.event_key(event_id);
        
        let event_json: Option<String> = conn.get(&event_key).await?;
        
        match event_json {
            Some(json) => {
                let event: AuditEvent = serde_json::from_str(&json)?;
                Ok(Some(event))
            }
            None => Ok(None),
        }
    }

    async fn get_events_by_timerange(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<AuditEvent>> {
        let mut conn = self.client.get_async_connection().await?;
        let mut events = Vec::new();
        
        // Generate date range
        let mut current_date = start.date_naive();
        let end_date = end.date_naive();
        
        while current_date <= end_date {
            let date_str = current_date.format("%Y-%m-%d").to_string();
            
            let index_key = match tenant_id {
                Some(tid) => self.tenant_index_key(tid, &date_str),
                None => self.global_index_key(&date_str),
            };
            
            let event_ids: Vec<String> = conn
                .zrangebyscore(
                    &index_key,
                    start.timestamp(),
                    end.timestamp(),
                )
                .await?;
            
            for event_id_str in event_ids {
                if let Ok(event_id) = Uuid::parse_str(&event_id_str) {
                    if let Some(event) = self.get_event(&event_id).await? {
                        events.push(event);
                    }
                }
            }
            
            current_date = current_date.succ_opt().unwrap_or(end_date);
        }
        
        // Sort by timestamp
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        
        Ok(events)
    }

    async fn get_events_by_actor(
        &self,
        actor_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<Vec<AuditEvent>> {
        let mut conn = self.client.get_async_connection().await?;
        let actor_index = self.actor_index_key(actor_id, tenant_id);
        
        let event_ids: Vec<String> = conn.zrange(&actor_index, 0, -1).await?;
        let mut events = Vec::new();
        
        for event_id_str in event_ids {
            if let Ok(event_id) = Uuid::parse_str(&event_id_str) {
                if let Some(event) = self.get_event(&event_id).await? {
                    events.push(event);
                }
            }
        }
        
        // Sort by timestamp
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        
        Ok(events)
    }

    async fn verify_integrity(&self) -> Result<bool> {
        // For Redis storage, we verify by checking if we can connect and perform basic operations
        let mut conn = self.client.get_async_connection().await?;
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        Ok(true)
    }
}

pub struct FileAuditStorage {
    file_path: String,
}

impl FileAuditStorage {
    pub fn new(file_path: String) -> Result<Self> {
        // Ensure the directory exists
        if let Some(parent) = Path::new(&file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        Ok(Self { file_path })
    }
}

#[async_trait]
impl AuditStorage for FileAuditStorage {
    async fn store_event(&self, event: &AuditEvent) -> Result<()> {
        let event_json = serde_json::to_string(event)?;
        let log_line = format!("{}\n", event_json);
        
        // Use blocking file operations in a spawn_blocking to avoid blocking the async runtime
        let file_path = self.file_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file_path)?;
            file.write_all(log_line.as_bytes())?;
            file.sync_all()?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;
        
        Ok(())
    }

    async fn get_event(&self, event_id: &Uuid) -> Result<Option<AuditEvent>> {
        let content = fs::read_to_string(&self.file_path).await?;
        
        for line in content.lines() {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(line) {
                if event.id == *event_id {
                    return Ok(Some(event));
                }
            }
        }
        
        Ok(None)
    }

    async fn get_events_by_timerange(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        tenant_id: Option<&str>,
    ) -> Result<Vec<AuditEvent>> {
        let content = fs::read_to_string(&self.file_path).await?;
        let mut events = Vec::new();
        
        for line in content.lines() {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(line) {
                if event.timestamp >= start && event.timestamp <= end {
                    if let Some(tid) = tenant_id {
                        if event.tenant_id.as_deref() == Some(tid) {
                            events.push(event);
                        }
                    } else {
                        events.push(event);
                    }
                }
            }
        }
        
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(events)
    }

    async fn get_events_by_actor(
        &self,
        actor_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<Vec<AuditEvent>> {
        let content = fs::read_to_string(&self.file_path).await?;
        let mut events = Vec::new();
        
        for line in content.lines() {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(line) {
                let matches_actor = event.actor.user_id.as_deref() == Some(actor_id)
                    || event.actor.api_key_id.as_deref() == Some(actor_id);
                
                if matches_actor {
                    if let Some(tid) = tenant_id {
                        if event.tenant_id.as_deref() == Some(tid) {
                            events.push(event);
                        }
                    } else {
                        events.push(event);
                    }
                }
            }
        }
        
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(events)
    }

    async fn verify_integrity(&self) -> Result<bool> {
        // For file storage, verify by checking if the file is readable and contains valid JSON
        if !Path::new(&self.file_path).exists() {
            return Ok(true); // Empty file is valid
        }
        
        let content = fs::read_to_string(&self.file_path).await?;
        
        for line in content.lines() {
            if !line.trim().is_empty() {
                serde_json::from_str::<AuditEvent>(line)?;
            }
        }
        
        Ok(true)
    }
}