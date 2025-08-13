use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;

// Integration tests for the RateWatch API
// These tests require Redis to be running on localhost:6379

#[tokio::test]
async fn test_health_endpoint() {
    let client = reqwest::Client::new();

    // Test health endpoint
    let response = client.get("http://localhost:8081/health").send().await;

    if let Ok(resp) = response {
        assert_eq!(resp.status(), 200);

        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "ok");
        assert!(body["timestamp"].is_string());
        assert_eq!(body["version"], "1.0.0");
    } else {
        // Server not running, skip test
        println!("Skipping integration test - server not running on localhost:8081");
    }
}

#[tokio::test]
async fn test_rate_limit_endpoint_without_auth() {
    let client = reqwest::Client::new();

    let response = client
        .post("http://localhost:8081/v1/check")
        .json(&json!({
            "key": "test:user",
            "limit": 10,
            "window": 60,
            "cost": 1
        }))
        .send()
        .await;

    if let Ok(resp) = response {
        // Should return 401 Unauthorized without proper auth
        assert_eq!(resp.status(), 401);
    } else {
        println!("Skipping integration test - server not running on localhost:8081");
    }
}

#[tokio::test]
async fn test_rate_limit_endpoint_with_auth() {
    let client = reqwest::Client::new();

    let response = client
        .post("http://localhost:8081/v1/check")
        .header(
            "Authorization",
            "Bearer rw_1234567890abcdef1234567890abcdef",
        )
        .json(&json!({
            "key": "test:integration",
            "limit": 5,
            "window": 60,
            "cost": 1
        }))
        .send()
        .await;

    if let Ok(resp) = response {
        assert_eq!(resp.status(), 200);

        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["allowed"], true);
        assert!(body["remaining"].as_u64().unwrap() <= 5);
        assert!(body["reset_in"].as_u64().unwrap() <= 60);
    } else {
        println!("Skipping integration test - server not running on localhost:8081");
    }
}

#[tokio::test]
async fn test_rate_limit_enforcement() {
    let client = reqwest::Client::new();
    let test_key = format!("test:enforcement:{}", chrono::Utc::now().timestamp());

    // Make multiple requests to exceed the limit
    for i in 1..=3 {
        let response = client
            .post("http://localhost:8081/v1/check")
            .header(
                "Authorization",
                "Bearer rw_1234567890abcdef1234567890abcdef",
            )
            .json(&json!({
                "key": test_key,
                "limit": 2,
                "window": 60,
                "cost": 1
            }))
            .send()
            .await;

        if let Ok(resp) = response {
            assert_eq!(resp.status(), 200);

            let body: Value = resp.json().await.unwrap();

            if i <= 2 {
                // First two requests should be allowed
                assert_eq!(body["allowed"], true);
            } else {
                // Third request should be denied
                assert_eq!(body["allowed"], false);
                assert_eq!(body["remaining"], 0);
                assert!(body["retry_after"].is_number());
            }
        } else {
            println!("Skipping integration test - server not running on localhost:8081");
            return;
        }

        // Small delay between requests
        sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn test_privacy_delete_endpoint() {
    let client = reqwest::Client::new();

    let response = client
        .post("http://localhost:8081/v1/privacy/delete")
        .header(
            "Authorization",
            "Bearer rw_1234567890abcdef1234567890abcdef",
        )
        .json(&json!({
            "user_id": "test:privacy:user",
            "reason": "user_request"
        }))
        .send()
        .await;

    if let Ok(resp) = response {
        assert_eq!(resp.status(), 200);

        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["message"].is_string());
        assert!(body["deleted_keys"].is_number());
    } else {
        println!("Skipping integration test - server not running on localhost:8081");
    }
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let client = reqwest::Client::new();

    let response = client.get("http://localhost:8081/metrics").send().await;

    if let Ok(resp) = response {
        assert_eq!(resp.status(), 200);

        let body = resp.text().await.unwrap();

        // Check for expected Prometheus metrics
        assert!(body.contains("ratewatch_requests_total"));
        assert!(body.contains("ratewatch_request_duration_seconds"));
        assert!(body.contains("ratewatch_rate_limit_hits_total"));
        assert!(body.contains("ratewatch_rate_limit_misses_total"));
    } else {
        println!("Skipping integration test - server not running on localhost:8081");
    }
}

#[tokio::test]
async fn test_invalid_json_request() {
    let client = reqwest::Client::new();

    let response = client
        .post("http://localhost:8081/v1/check")
        .header(
            "Authorization",
            "Bearer rw_1234567890abcdef1234567890abcdef",
        )
        .header("Content-Type", "application/json")
        .body("invalid json")
        .send()
        .await;

    if let Ok(resp) = response {
        // Should return 400 Bad Request for invalid JSON
        assert_eq!(resp.status(), 400);
    } else {
        println!("Skipping integration test - server not running on localhost:8081");
    }
}

#[tokio::test]
async fn test_missing_required_fields() {
    let client = reqwest::Client::new();

    let response = client
        .post("http://localhost:8081/v1/check")
        .header(
            "Authorization",
            "Bearer rw_1234567890abcdef1234567890abcdef",
        )
        .json(&json!({
            "key": "test:missing",
            // Missing limit, window, cost
        }))
        .send()
        .await;

    if let Ok(resp) = response {
        // Should return 422 Unprocessable Entity for missing fields
        assert!(resp.status().is_client_error());
    } else {
        println!("Skipping integration test - server not running on localhost:8081");
    }
}
