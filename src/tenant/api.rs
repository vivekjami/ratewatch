use super::{TenantManager, TenantOnboardingRequest, TenantConfig, TenantSettings, ResourceQuotas};
use super::isolation::{IsolationLevel, DataClassification};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    pub slug: String,
    pub admin_email: String,
    pub organization: String,
    pub isolation_level: Option<IsolationLevel>,
    pub data_classification: Option<DataClassification>,
    pub initial_quotas: Option<ResourceQuotas>,
    pub initial_settings: Option<TenantSettings>,
    pub features: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub settings: Option<TenantSettings>,
    pub quotas: Option<ResourceQuotas>,
    pub features: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct ListTenantsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TenantResponse {
    pub tenant: TenantConfig,
    pub quota_violations: Vec<super::resource_quota::QuotaViolation>,
    pub health_status: bool,
}

#[derive(Debug, Serialize)]
pub struct TenantsListResponse {
    pub tenants: Vec<TenantConfig>,
    pub total: usize,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct SuspendTenantRequest {
    pub reason: String,
}

pub type TenantManagerState = Arc<Mutex<TenantManager>>;

pub fn create_tenant_routes() -> Router<TenantManagerState> {
    Router::new()
        .route("/tenants", post(create_tenant))
        .route("/tenants", get(list_tenants))
        .route("/tenants/:tenant_id", get(get_tenant))
        .route("/tenants/:tenant_id", put(update_tenant))
        .route("/tenants/:tenant_id", delete(delete_tenant))
        .route("/tenants/:tenant_id/suspend", post(suspend_tenant))
        .route("/tenants/:tenant_id/reactivate", post(reactivate_tenant))
        .route("/tenants/:tenant_id/health", get(health_check_tenant))
        .route("/tenants/:tenant_id/quotas", get(get_tenant_quotas))
        .route("/tenants/slug/:slug", get(get_tenant_by_slug))
}

async fn create_tenant(
    State(tenant_manager): State<TenantManagerState>,
    Json(request): Json<CreateTenantRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let onboarding_request = TenantOnboardingRequest {
        name: request.name,
        slug: request.slug,
        admin_email: request.admin_email,
        organization: request.organization,
        isolation_level: request.isolation_level.unwrap_or(IsolationLevel::Shared),
        data_classification: request.data_classification.unwrap_or(DataClassification::Internal),
        initial_quotas: request.initial_quotas,
        initial_settings: request.initial_settings,
        features: request.features.unwrap_or_default(),
        metadata: request.metadata.unwrap_or_default(),
    };

    let mut manager = tenant_manager.lock().await;
    match manager.create_tenant(onboarding_request).await {
        Ok(tenant_id) => {
            let response = serde_json::json!({
                "tenant_id": tenant_id,
                "status": "provisioning",
                "message": "Tenant creation initiated"
            });
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to create tenant: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

async fn list_tenants(
    State(tenant_manager): State<TenantManagerState>,
    Query(query): Query<ListTenantsQuery>,
) -> Result<Json<TenantsListResponse>, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.list_tenants(query.limit, query.offset).await {
        Ok(tenants) => {
            let filtered_tenants = if let Some(status_filter) = &query.status {
                tenants.into_iter()
                    .filter(|t| format!("{:?}", t.status).to_lowercase() == status_filter.to_lowercase())
                    .collect()
            } else {
                tenants
            };

            let response = TenantsListResponse {
                total: filtered_tenants.len(),
                tenants: filtered_tenants,
                limit: query.limit,
                offset: query.offset,
            };
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to list tenants: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_tenant(
    State(tenant_manager): State<TenantManagerState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<TenantResponse>, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.get_tenant_config(tenant_id).await {
        Ok(tenant) => {
            let quota_violations = manager.check_quota_violations(tenant_id).await
                .unwrap_or_default();
            
            let health_status = manager.health_check_tenant(tenant_id).await
                .unwrap_or(false);

            let response = TenantResponse {
                tenant,
                quota_violations,
                health_status,
            };
            Ok(Json(response))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_tenant_by_slug(
    State(tenant_manager): State<TenantManagerState>,
    Path(slug): Path<String>,
) -> Result<Json<TenantResponse>, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.get_tenant_by_slug(&slug).await {
        Ok(Some(tenant)) => {
            let quota_violations = manager.check_quota_violations(tenant.id).await
                .unwrap_or_default();
            
            let health_status = manager.health_check_tenant(tenant.id).await
                .unwrap_or(false);

            let response = TenantResponse {
                tenant,
                quota_violations,
                health_status,
            };
            Ok(Json(response))
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get tenant by slug: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn update_tenant(
    State(tenant_manager): State<TenantManagerState>,
    Path(tenant_id): Path<Uuid>,
    Json(request): Json<UpdateTenantRequest>,
) -> Result<Json<TenantConfig>, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.get_tenant_config(tenant_id).await {
        Ok(mut tenant) => {
            if let Some(name) = request.name {
                tenant.name = name;
            }
            
            if let Some(settings) = request.settings {
                tenant.update_settings(settings);
            }
            
            if let Some(quotas) = request.quotas {
                tenant.update_quotas(quotas);
            }
            
            if let Some(features) = request.features {
                tenant.features = features;
            }
            
            if let Some(metadata) = request.metadata {
                tenant.metadata.extend(metadata);
            }

            match manager.update_tenant_config(tenant_id, tenant.clone()).await {
                Ok(_) => Ok(Json(tenant)),
                Err(e) => {
                    tracing::error!("Failed to update tenant: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn delete_tenant(
    State(tenant_manager): State<TenantManagerState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.delete_tenant(tenant_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            tracing::error!("Failed to delete tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn suspend_tenant(
    State(tenant_manager): State<TenantManagerState>,
    Path(tenant_id): Path<Uuid>,
    Json(request): Json<SuspendTenantRequest>,
) -> Result<StatusCode, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.suspend_tenant(tenant_id, request.reason).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => {
            tracing::error!("Failed to suspend tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn reactivate_tenant(
    State(tenant_manager): State<TenantManagerState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.reactivate_tenant(tenant_id).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => {
            tracing::error!("Failed to reactivate tenant: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn health_check_tenant(
    State(tenant_manager): State<TenantManagerState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.health_check_tenant(tenant_id).await {
        Ok(healthy) => {
            let response = serde_json::json!({
                "tenant_id": tenant_id,
                "healthy": healthy,
                "timestamp": chrono::Utc::now().to_rfc3339()
            });
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to check tenant health: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_tenant_quotas(
    State(tenant_manager): State<TenantManagerState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut manager = tenant_manager.lock().await;
    
    match manager.get_tenant_config(tenant_id).await {
        Ok(tenant) => {
            match manager.check_quota_violations(tenant_id).await {
                Ok(violations) => {
                    let response = serde_json::json!({
                        "tenant_id": tenant_id,
                        "quotas": tenant.quotas,
                        "violations": violations,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    });
                    Ok(Json(response))
                }
                Err(e) => {
                    tracing::error!("Failed to check quota violations: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}