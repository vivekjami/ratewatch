use once_cell::sync::Lazy;
use prometheus::{
    Counter, Histogram, IntCounter, IntGauge, Registry, TextEncoder,
    HistogramOpts,
};
use axum::{
    http::StatusCode,
    response::Response,
    routing::get,
    Router,
};

// Global metrics
pub static REGISTRY: Lazy<Registry> = Lazy::new(|| {
    let registry = Registry::new();
    
    // Register default metrics
    registry.register(Box::new(REQUEST_TOTAL.clone())).unwrap();
    registry.register(Box::new(REQUEST_DURATION.clone())).unwrap();
    registry.register(Box::new(RATE_LIMIT_HITS.clone())).unwrap();
    registry.register(Box::new(RATE_LIMIT_MISSES.clone())).unwrap();
    registry.register(Box::new(ACTIVE_CONNECTIONS.clone())).unwrap();
    registry.register(Box::new(REDIS_OPERATIONS.clone())).unwrap();
    
    registry
});

pub static REQUEST_TOTAL: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new("ratewatch_requests_total", "Total number of requests")
        .expect("metric can be created")
});

pub static REQUEST_DURATION: Lazy<Histogram> = Lazy::new(|| {
    Histogram::with_opts(
        HistogramOpts::new("ratewatch_request_duration_seconds", "Request duration in seconds")
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0])
    ).expect("metric can be created")
});

pub static RATE_LIMIT_HITS: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new("ratewatch_rate_limit_hits_total", "Total number of rate limit hits")
        .expect("metric can be created")
});

pub static RATE_LIMIT_MISSES: Lazy<IntCounter> = Lazy::new(|| {
    IntCounter::new("ratewatch_rate_limit_misses_total", "Total number of rate limit misses")
        .expect("metric can be created")
});

pub static ACTIVE_CONNECTIONS: Lazy<IntGauge> = Lazy::new(|| {
    IntGauge::new("ratewatch_active_connections", "Number of active connections")
        .expect("metric can be created")
});

pub static REDIS_OPERATIONS: Lazy<Counter> = Lazy::new(|| {
    Counter::new("ratewatch_redis_operations_total", "Total number of Redis operations")
        .expect("metric can be created")
});

pub fn create_metrics_router() -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
}

async fn metrics_handler() -> Result<Response<String>, StatusCode> {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    
    match encoder.encode_to_string(&metric_families) {
        Ok(output) => {
            let response = Response::builder()
                .status(200)
                .header("content-type", "text/plain; version=0.0.4")
                .body(output)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(response)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
