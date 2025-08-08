# RateWatch API Documentation

## Authentication

All API endpoints require authentication via API key in the Authorization header:

```
Authorization: Bearer your-api-key-here
```

## Rate Limiting Endpoints

### Check Rate Limit

Check if a request should be allowed based on rate limiting rules.

**Endpoint:** `POST /v1/check`

**Request Body:**
```json
{
  "key": "string",      // Unique identifier (user ID, IP, etc.)
  "limit": 100,         // Maximum requests allowed
  "window": 3600,       // Time window in seconds
  "cost": 1             // Cost of this request (optional, default: 1)
}
```

**Response:**
```json
{
  "allowed": true,      // Whether request should be allowed
  "remaining": 99,      // Requests remaining in window
  "reset_in": 3542,     // Seconds until window resets
  "retry_after": null   // Seconds to wait before retry (if denied)
}
```

**Example:**
```bash
curl -X POST https://api.ratewatch.dev/v1/check \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "key": "user:123",
    "limit": 100,
    "window": 3600,
    "cost": 1
  }'
```

## Analytics Endpoints

### Get Statistics

Get aggregated statistics about rate limiting usage.

**Endpoint:** `GET /v1/analytics/stats`

**Response:**
```json
{
  "total_requests": 12345,
  "allowed_requests": 11234,
  "denied_requests": 1111,
  "success_rate": 91.0,
  "avg_response_time_ms": 2.3,
  "unique_keys": 456
}
```

### Get Top Keys

Get the most active rate limiting keys.

**Endpoint:** `GET /v1/analytics/top-keys`

**Query Parameters:**
- `limit` (optional): Number of keys to return (default: 10)
- `window` (optional): Time window in hours (default: 24)

**Response:**
```json
{
  "keys": [
    {
      "key": "user:123",
      "requests": 789,
      "denied": 12,
      "success_rate": 98.5
    }
  ]
}
```

### Get Activity Log

Get recent activity and events.

**Endpoint:** `GET /v1/analytics/activity`

**Query Parameters:**
- `limit` (optional): Number of entries to return (default: 50)
- `severity` (optional): Filter by severity (info, warning, error)

**Response:**
```json
{
  "activities": [
    {
      "timestamp": "2025-08-08T10:30:00Z",
      "message": "Rate limit exceeded for key: user:456",
      "severity": "warning",
      "key": "user:456"
    }
  ]
}
```

## Privacy & GDPR Endpoints

### Delete User Data

Delete all data associated with a user (GDPR compliance).

**Endpoint:** `POST /v1/privacy/delete`

**Request Body:**
```json
{
  "user_id": "user:123",
  "reason": "user_request"
}
```

**Response:**
```json
{
  "success": true,
  "message": "Successfully deleted data for user user:123",
  "deleted_keys": 5
}
```

### Get User Data Summary

Get a summary of data stored for a user.

**Endpoint:** `POST /v1/privacy/summary`

**Request Body:**
```json
{
  "user_id": "user:123"
}
```

**Response:**
```json
{
  "user_id": "user:123",
  "total_keys": 3,
  "total_requests": 147,
  "active_windows": 2,
  "data_retention_days": 30
}
```

## Health & Monitoring

### Basic Health Check

**Endpoint:** `GET /health`

**Response:**
```json
{
  "status": "ok",
  "timestamp": "2025-08-08T10:30:00Z",
  "version": "0.1.0"
}
```

### Detailed Health Check

**Endpoint:** `GET /health/detailed`

**Response:**
```json
{
  "status": "ok",
  "timestamp": "2025-08-08T10:30:00Z",
  "version": "0.1.0",
  "uptime": "99.9%",
  "dependencies": {
    "redis": {
      "status": "healthy",
      "latency_ms": 1
    },
    "api": {
      "status": "healthy"
    }
  }
}
```

### Prometheus Metrics

**Endpoint:** `GET /metrics`

Returns Prometheus-formatted metrics for monitoring:

- `ratewatch_requests_total` - Total number of requests
- `ratewatch_request_duration_seconds` - Request duration histogram
- `ratewatch_rate_limit_hits_total` - Total rate limit hits (allowed)
- `ratewatch_rate_limit_misses_total` - Total rate limit misses (denied)
- `ratewatch_active_connections` - Current active connections
- `ratewatch_redis_operations_total` - Total Redis operations

## Error Handling

### HTTP Status Codes

- `200` - Success
- `400` - Bad Request (invalid request format)
- `401` - Unauthorized (invalid or missing API key)
- `429` - Too Many Requests (rate limited)
- `500` - Internal Server Error

### Error Response Format

```json
{
  "error": "invalid_request",
  "message": "Missing required field: key",
  "code": 400
}
```

## Rate Limiting Algorithm

RateWatch uses a sliding window algorithm with Redis for distributed rate limiting:

1. **Window Calculation**: Current time divided by window size
2. **Key Generation**: `rate_limit:{user_key}:{window_start}`
3. **Atomic Check**: Redis INCR operation for thread safety
4. **TTL Management**: Automatic expiration of old windows

## Best Practices

### Choosing Window Sizes

- **Short bursts**: 60 seconds for API endpoints
- **Hourly limits**: 3600 seconds for user quotas
- **Daily limits**: 86400 seconds for subscription tiers

### Key Naming

Use hierarchical naming for better analytics:

```
user:123           # Per-user limits
api:endpoint:v1    # Per-endpoint limits
ip:192.168.1.1     # Per-IP limits
tenant:acme-corp   # Per-tenant limits
```

### Cost-based Limiting

Use cost to implement weighted rate limiting:

```json
{
  "key": "user:123",
  "limit": 1000,
  "window": 3600,
  "cost": 10    // Expensive operation costs 10 "points"
}
```

### Error Handling

Always handle rate limit responses gracefully:

```javascript
const response = await ratewatch.check('user:123', {limit: 100, window: 3600});

if (!response.allowed) {
  // Wait before retrying
  await sleep(response.retry_after * 1000);
  // Or show user a friendly message
  throw new Error(`Rate limited. Try again in ${response.retry_after} seconds`);
}
```

## Client Libraries

### Python

```python
from ratewatch import RateWatchClient

client = RateWatchClient('your-api-key')
result = client.check('user:123', limit=100, window=3600)

if not result.allowed:
    raise Exception(f"Rate limited. Reset in {result.reset_in} seconds")
```

### Node.js

```javascript
import { RateWatchClient } from '@ratewatch/client';

const client = new RateWatchClient('your-api-key');
const result = await client.check('user:123', {limit: 100, window: 3600});

if (!result.allowed) {
    throw new Error(`Rate limited. Reset in ${result.resetIn} seconds`);
}
```

### Rust

```rust
use ratewatch::Client;

let client = Client::new("your-api-key")?;
let result = client.check("user:123", 100, 3600, 1).await?;

if !result.allowed {
    return Err(format!("Rate limited. Reset in {} seconds", result.reset_in));
}
```

## Webhooks

Configure webhooks to receive notifications about rate limiting events:

```json
{
  "url": "https://your-app.com/webhooks/ratewatch",
  "events": ["rate_limit_exceeded", "suspicious_activity"],
  "secret": "webhook-secret-for-verification"
}
```

## SDKs and Integrations

### Framework Middleware

**Express.js:**
```javascript
const ratewatch = require('@ratewatch/express');
app.use(ratewatch({
  apiKey: 'your-api-key',
  keyGenerator: (req) => req.user.id,
  limit: 100,
  window: 3600
}));
```

**Django:**
```python
from ratewatch.django import RateWatchMiddleware

MIDDLEWARE = [
    'ratewatch.django.RateWatchMiddleware',
    # ... other middleware
]

RATEWATCH_API_KEY = 'your-api-key'
RATEWATCH_DEFAULT_LIMIT = 100
RATEWATCH_DEFAULT_WINDOW = 3600
```

### API Gateways

**Kong Plugin:**
```yaml
plugins:
- name: ratewatch
  config:
    api_key: your-api-key
    default_limit: 100
    default_window: 3600
```

**Envoy Filter:**
```yaml
http_filters:
- name: envoy.filters.http.ratewatch
  typed_config:
    "@type": type.googleapis.com/envoy.extensions.filters.http.ratewatch.v3.RateWatch
    api_key: your-api-key
```
