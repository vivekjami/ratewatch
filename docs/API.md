# RateWatch API Documentation

## Authentication

All API endpoints require authentication using an API key in the Authorization header:

```
Authorization: Bearer your-api-key-here
```

## Endpoints

### Rate Limiting

#### POST /v1/check
Check if a request should be allowed based on rate limiting rules.

**Request:**
```json
{
  "key": "user:123",
  "limit": 100,
  "window": 3600,
  "cost": 1
}
```

**Response:**
```json
{
  "allowed": true,
  "remaining": 99,
  "reset_time": 1642694400,
  "retry_after": null
}
```

### Privacy (GDPR Compliance)

#### GET /v1/privacy/summary
Get a summary of stored data for a user.

**Query Parameters:**
- `user_id`: User identifier

**Response:**
```json
{
  "user_id": "user:123",
  "data_points": 5,
  "oldest_entry": "2024-01-01T00:00:00Z",
  "newest_entry": "2024-01-01T12:00:00Z"
}
```

#### POST /v1/privacy/delete
Delete all data for a user (Right to Erasure).

**Request:**
```json
{
  "user_id": "user:123",
  "reason": "User requested deletion"
}
```

### Analytics

#### GET /v1/analytics/stats
Get rate limiting statistics.

**Response:**
```json
{
  "total_requests": 1000,
  "allowed_requests": 950,
  "denied_requests": 50,
  "top_keys": [
    {"key": "user:123", "count": 100},
    {"key": "user:456", "count": 75}
  ]
}
```

### System

#### GET /health
Health check endpoint.

**Response:**
```json
{
  "status": "ok",
  "timestamp": "2024-01-01T12:00:00Z",
  "version": "1.0.0"
}
```

#### GET /metrics
Prometheus metrics endpoint.

**Response:** Prometheus format metrics

## Error Responses

All endpoints return appropriate HTTP status codes and error messages:

```json
{
  "error": "Invalid API key",
  "code": "INVALID_AUTH"
}
```

## Rate Limiting

The API itself is rate limited to prevent abuse. Rate limits are applied per API key.