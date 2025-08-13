use std::time::{Duration, Instant};
use tokio::time::sleep;

// Performance tests to validate <500ms response time requirement
// These tests require Redis to be running

#[tokio::test]
async fn test_response_time_under_500ms() {
    let client = reqwest::Client::new();

    let mut total_time = Duration::new(0, 0);
    let mut successful_requests = 0;
    const NUM_REQUESTS: usize = 10;

    for i in 0..NUM_REQUESTS {
        let start = Instant::now();

        let response = client
            .post("http://localhost:8081/v1/check")
            .header(
                "Authorization",
                "Bearer rw_1234567890abcdef1234567890abcdef",
            )
            .json(&serde_json::json!({
                "key": format!("perf:test:{}", i),
                "limit": 1000,
                "window": 3600,
                "cost": 1
            }))
            .send()
            .await;

        let elapsed = start.elapsed();

        if let Ok(resp) = response {
            if resp.status().is_success() {
                total_time += elapsed;
                successful_requests += 1;

                // Each individual request should be under 500ms
                assert!(
                    elapsed < Duration::from_millis(500),
                    "Request {} took {}ms, exceeding 500ms limit",
                    i,
                    elapsed.as_millis()
                );
            }
        } else {
            println!("Skipping performance test - server not running on localhost:8081");
            return;
        }

        // Small delay to avoid overwhelming the server
        sleep(Duration::from_millis(10)).await;
    }

    if successful_requests > 0 {
        let avg_time = total_time / successful_requests as u32;
        println!(
            "Average response time: {}ms (target: <500ms)",
            avg_time.as_millis()
        );

        // Average should also be well under 500ms
        assert!(
            avg_time < Duration::from_millis(500),
            "Average response time {}ms exceeds 500ms target",
            avg_time.as_millis()
        );
    }
}

#[tokio::test]
async fn test_concurrent_requests_performance() {
    use tokio::task::JoinSet;

    let client = reqwest::Client::new();
    let mut join_set = JoinSet::new();

    const CONCURRENT_REQUESTS: usize = 20;
    let start_time = Instant::now();

    // Launch concurrent requests
    for i in 0..CONCURRENT_REQUESTS {
        let client_clone = client.clone();
        join_set.spawn(async move {
            let request_start = Instant::now();

            let response = client_clone
                .post("http://localhost:8081/v1/check")
                .header(
                    "Authorization",
                    "Bearer rw_1234567890abcdef1234567890abcdef",
                )
                .json(&serde_json::json!({
                    "key": format!("concurrent:test:{}", i),
                    "limit": 100,
                    "window": 60,
                    "cost": 1
                }))
                .send()
                .await;

            let elapsed = request_start.elapsed();
            (i, response, elapsed)
        });
    }

    let mut successful_requests = 0;
    let mut max_time = Duration::new(0, 0);
    let mut total_time = Duration::new(0, 0);

    // Collect results
    while let Some(result) = join_set.join_next().await {
        if let Ok((i, response, elapsed)) = result {
            if let Ok(resp) = response {
                if resp.status().is_success() {
                    successful_requests += 1;
                    total_time += elapsed;
                    max_time = max_time.max(elapsed);

                    // Even under concurrent load, each request should be under 500ms
                    assert!(
                        elapsed < Duration::from_millis(500),
                        "Concurrent request {} took {}ms, exceeding 500ms limit",
                        i,
                        elapsed.as_millis()
                    );
                }
            } else {
                println!("Skipping concurrent performance test - server not running");
                return;
            }
        }
    }

    let total_elapsed = start_time.elapsed();

    if successful_requests > 0 {
        let avg_time = total_time / successful_requests as u32;

        println!(
            "Concurrent test results: {} requests in {}ms",
            successful_requests,
            total_elapsed.as_millis()
        );
        println!("Average response time: {}ms", avg_time.as_millis());
        println!("Max response time: {}ms", max_time.as_millis());

        // Under concurrent load, average should still be reasonable
        assert!(
            avg_time < Duration::from_millis(500),
            "Average concurrent response time {}ms exceeds 500ms target",
            avg_time.as_millis()
        );

        // Max time should also be reasonable
        assert!(
            max_time < Duration::from_millis(1000),
            "Max concurrent response time {}ms exceeds 1000ms limit",
            max_time.as_millis()
        );
    }
}

#[tokio::test]
async fn test_memory_usage_stability() {
    // This test makes many requests to check for memory leaks
    let client = reqwest::Client::new();

    const STRESS_REQUESTS: usize = 100;
    let mut successful_requests = 0;

    for i in 0..STRESS_REQUESTS {
        let response = client
            .post("http://localhost:8081/v1/check")
            .header(
                "Authorization",
                "Bearer rw_1234567890abcdef1234567890abcdef",
            )
            .json(&serde_json::json!({
                "key": format!("stress:test:{}", i % 10), // Reuse keys to test cleanup
                "limit": 50,
                "window": 60,
                "cost": 1
            }))
            .send()
            .await;

        if let Ok(resp) = response {
            if resp.status().is_success() {
                successful_requests += 1;
            }
        } else {
            println!("Skipping memory stability test - server not running");
            return;
        }

        // No delay - stress test
    }

    println!(
        "Memory stability test: {} successful requests out of {}",
        successful_requests, STRESS_REQUESTS
    );

    // Should handle all requests successfully
    assert!(
        successful_requests >= STRESS_REQUESTS * 95 / 100,
        "Only {}/{} requests succeeded, indicating potential memory issues",
        successful_requests,
        STRESS_REQUESTS
    );
}
