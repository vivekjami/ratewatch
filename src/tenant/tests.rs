use super::*;
use super::tenant_manager::{TenantManager, TenantOnboardingRequest};
use super::isolation::{IsolationLevel, DataClassification};
use uuid::Uuid;
use std::collections::HashMap;

#[tokio::test]
async fn test_tenant_creation_and_retrieval() {
    let redis_url = "redis://127.0.0.1:6379";
    let mut tenant_manager = TenantManager::new(redis_url, "test".to_string()).unwrap();

    let request = TenantOnboardingRequest {
        name: "Test Tenant".to_string(),
        slug: "test-tenant".to_string(),
        admin_email: "admin@test.com".to_string(),
        organization: "Test Org".to_string(),
        isolation_level: IsolationLevel::Shared,
        data_classification: DataClassification::Internal,
        initial_quotas: None,
        initial_settings: None,
        features: vec!["analytics".to_string()],
        metadata: HashMap::new(),
    };

    let tenant_id = tenant_manager.create_tenant(request).await.unwrap();
    let retrieved_tenant = tenant_manager.get_tenant_config(tenant_id).await.unwrap();

    assert_eq!(retrieved_tenant.name, "Test Tenant");
    assert_eq!(retrieved_tenant.slug, "test-tenant");
    assert!(retrieved_tenant.can_access_feature("analytics"));

    // Cleanup
    tenant_manager.delete_tenant(tenant_id).await.unwrap();
}

#[tokio::test]
async fn test_tenant_slug_uniqueness() {
    let redis_url = "redis://127.0.0.1:6379";
    let mut tenant_manager = TenantManager::new(redis_url, "test".to_string()).unwrap();

    let request1 = TenantOnboardingRequest {
        name: "Test Tenant 1".to_string(),
        slug: "unique-slug".to_string(),
        admin_email: "admin1@test.com".to_string(),
        organization: "Test Org 1".to_string(),
        isolation_level: IsolationLevel::Shared,
        data_classification: DataClassification::Internal,
        initial_quotas: None,
        initial_settings: None,
        features: vec![],
        metadata: HashMap::new(),
    };

    let request2 = TenantOnboardingRequest {
        name: "Test Tenant 2".to_string(),
        slug: "unique-slug".to_string(), // Same slug
        admin_email: "admin2@test.com".to_string(),
        organization: "Test Org 2".to_string(),
        isolation_level: IsolationLevel::Shared,
        data_classification: DataClassification::Internal,
        initial_quotas: None,
        initial_settings: None,
        features: vec![],
        metadata: HashMap::new(),
    };

    let tenant_id1 = tenant_manager.create_tenant(request1).await.unwrap();
    let result2 = tenant_manager.create_tenant(request2).await;

    assert!(result2.is_err());

    // Cleanup
    tenant_manager.delete_tenant(tenant_id1).await.unwrap();
}

#[tokio::test]
async fn test_tenant_quota_management() {
    let redis_url = "redis://127.0.0.1:6379";
    let mut tenant_manager = TenantManager::new(redis_url, "test".to_string()).unwrap();

    let custom_quotas = ResourceQuotas {
        max_api_calls_per_hour: 100,
        max_storage_mb: 50,
        max_concurrent_requests: 10,
        max_users: 5,
        max_data_export_mb: 25,
    };

    let request = TenantOnboardingRequest {
        name: "Quota Test Tenant".to_string(),
        slug: "quota-test".to_string(),
        admin_email: "admin@test.com".to_string(),
        organization: "Test Org".to_string(),
        isolation_level: IsolationLevel::Shared,
        data_classification: DataClassification::Internal,
        initial_quotas: Some(custom_quotas.clone()),
        initial_settings: None,
        features: vec![],
        metadata: HashMap::new(),
    };

    let tenant_id = tenant_manager.create_tenant(request).await.unwrap();
    let tenant_config = tenant_manager.get_tenant_config(tenant_id).await.unwrap();

    assert_eq!(tenant_config.quotas.max_api_calls_per_hour, 100);
    assert_eq!(tenant_config.quotas.max_storage_mb, 50);

    // Test quota violation checking
    let violations = tenant_manager.check_quota_violations(tenant_id).await.unwrap();
    assert!(violations.is_empty()); // Should be empty for new tenant

    // Cleanup
    tenant_manager.delete_tenant(tenant_id).await.unwrap();
}

#[tokio::test]
async fn test_tenant_isolation() {
    let redis_url = "redis://127.0.0.1:6379";
    let isolation_manager = TenantIsolationManager::new(redis_url, "test".to_string()).unwrap();

    let tenant1_id = Uuid::new_v4();
    let tenant2_id = Uuid::new_v4();

    let context1 = isolation_manager.create_tenant_context(
        tenant1_id,
        IsolationLevel::Shared,
        DataClassification::Internal,
    );

    let context2 = isolation_manager.create_tenant_context(
        tenant2_id,
        IsolationLevel::Shared,
        DataClassification::Internal,
    );

    // Set data for tenant 1
    isolation_manager
        .set_tenant_data(&context1, "test_key", "tenant1_value", None)
        .await
        .unwrap();

    // Set data for tenant 2
    isolation_manager
        .set_tenant_data(&context2, "test_key", "tenant2_value", None)
        .await
        .unwrap();

    // Verify isolation - each tenant should only see their own data
    let tenant1_data = isolation_manager
        .get_tenant_data(&context1, "test_key")
        .await
        .unwrap();
    let tenant2_data = isolation_manager
        .get_tenant_data(&context2, "test_key")
        .await
        .unwrap();

    assert_eq!(tenant1_data, Some("tenant1_value".to_string()));
    assert_eq!(tenant2_data, Some("tenant2_value".to_string()));

    // Cleanup
    isolation_manager.purge_tenant_data(&context1).await.unwrap();
    isolation_manager.purge_tenant_data(&context2).await.unwrap();
}

#[tokio::test]
async fn test_tenant_suspension_and_reactivation() {
    let redis_url = "redis://127.0.0.1:6379";
    let mut tenant_manager = TenantManager::new(redis_url, "test".to_string()).unwrap();

    let request = TenantOnboardingRequest {
        name: "Suspension Test".to_string(),
        slug: "suspension-test".to_string(),
        admin_email: "admin@test.com".to_string(),
        organization: "Test Org".to_string(),
        isolation_level: IsolationLevel::Shared,
        data_classification: DataClassification::Internal,
        initial_quotas: None,
        initial_settings: None,
        features: vec![],
        metadata: HashMap::new(),
    };

    let tenant_id = tenant_manager.create_tenant(request).await.unwrap();
    
    // Activate tenant first
    let mut config = tenant_manager.get_tenant_config(tenant_id).await.unwrap();
    config.activate();
    tenant_manager.update_tenant_config(tenant_id, config).await.unwrap();

    // Verify tenant is active
    let config = tenant_manager.get_tenant_config(tenant_id).await.unwrap();
    assert!(config.is_active());

    // Suspend tenant
    tenant_manager
        .suspend_tenant(tenant_id, "Testing suspension".to_string())
        .await
        .unwrap();

    let suspended_config = tenant_manager.get_tenant_config(tenant_id).await.unwrap();
    assert!(!suspended_config.is_active());
    assert_eq!(suspended_config.status, TenantStatus::Suspended);

    // Reactivate tenant
    tenant_manager.reactivate_tenant(tenant_id).await.unwrap();

    let reactivated_config = tenant_manager.get_tenant_config(tenant_id).await.unwrap();
    assert!(reactivated_config.is_active());

    // Cleanup
    tenant_manager.delete_tenant(tenant_id).await.unwrap();
}

#[tokio::test]
async fn test_cross_tenant_access_validation() {
    let redis_url = "redis://127.0.0.1:6379";
    let isolation_manager = TenantIsolationManager::new(redis_url, "test".to_string()).unwrap();

    let tenant1_id = Uuid::new_v4();
    let tenant2_id = Uuid::new_v4();

    // Create private tenant context
    let private_context = isolation_manager.create_tenant_context(
        tenant1_id,
        IsolationLevel::Private,
        DataClassification::Restricted,
    );

    // Private tenants should not be able to access other tenant data
    let can_access = isolation_manager
        .validate_cross_tenant_access(&private_context, tenant2_id)
        .await
        .unwrap();

    assert!(!can_access);

    // Create shared tenant context
    let shared_context = isolation_manager.create_tenant_context(
        tenant1_id,
        IsolationLevel::Shared,
        DataClassification::Internal,
    );

    // Shared tenants should be able to access their own data
    let can_access_own = isolation_manager
        .validate_cross_tenant_access(&shared_context, tenant1_id)
        .await
        .unwrap();

    assert!(can_access_own);
}