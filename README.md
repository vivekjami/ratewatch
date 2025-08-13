# RateWatch

High-performance distributed API rate limiting service built with Rust.

## Features

- **Sub-500ms response time** - Optimized for low latency
- **Redis-based sliding window** - Distributed rate limiting with automatic expiration
- **GDPR compliant** - Built-in data deletion and privacy controls
- **Production ready** - Comprehensive security, monitoring, and error handling
- **Multiple client libraries** - Python, Node.js/TypeScript support

## Quick Start

### Installation

```bash
# Python
pip install ratewatch

# Node.js
npm install @ratewatch/client
```

### Usage

```python
# Python
from ratewatch import RateWatch

client = RateWatch(api_key="your-api-key")
result = client.check("user:123", limit=100, window=3600)

if result.allowed:
    print(f"Request allowed. {result.remaining} remaining.")
else:
    print(f"Rate limited. Retry in {result.retry_after}s")
```

```javascript
// Node.js/TypeScript
import { RateWatch } from '@ratewatch/client';

const client = new RateWatch('your-api-key');
const result = await client.check('user:123', 100, 3600);

if (result.allowed) {
    console.log(`Request allowed. ${result.remaining} remaining.`);
} else {
    console.log(`Rate limited. Retry in ${result.retryAfter}s`);
}
```

## API Reference

### Rate Limit Check

```http
POST /v1/check
Authorization: Bearer {api_key}
Content-Type: application/json

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
  "reset_in": 3542,
  "retry_after": null
}
```

### GDPR Data Deletion

```http
POST /v1/privacy/delete
Authorization: Bearer {api_key}
Content-Type: application/json

{
  "user_id": "user:123",
  "reason": "user_request"
}
```

### Health Check

```http
GET /health
```

**Response:**
```json
{
  "status": "ok",
  "timestamp": "2025-08-13T15:45:07Z",
  "version": "1.0.0"
}
```

## Deployment

### Docker

```bash
docker run -p 8081:8081 \
  -e REDIS_URL=redis://localhost:6379 \
  -e API_KEY_SECRET=your-secret \
  ratewatch/ratewatch:latest
```

### Docker Compose

```yaml
version: '3.8'
services:
  ratewatch:
    image: ratewatch/ratewatch:latest
    ports:
      - "8081:8081"
    environment:
      - REDIS_URL=redis://redis:6379
      - API_KEY_SECRET=your-secret-key
    depends_on:
      - redis
  
  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
```

### Environment Variables

- `REDIS_URL` - Redis connection URL (default: `redis://127.0.0.1:6379`)
- `PORT` - Server port (default: `8081`)
- `API_KEY_SECRET` - Secret for API key validation (required in production)

## Development

### Prerequisites

- Rust 1.70+
- Redis 6.0+

### Setup

```bash
git clone https://github.com/ratewatch/ratewatch.git
cd ratewatch

# Start Redis
docker-compose up -d redis

# Run the server
cargo run
```

### Testing

```bash
# Run all tests
cargo test

# Test client libraries
cd clients/python && python3 test_client.py
cd clients/nodejs && npm run build && node test_client.js
```

## Client Libraries

### Python Client

```python
from ratewatch import RateWatch, RateLimitExceeded

client = RateWatch(api_key="your-key")

try:
    result = client.check_with_exceptions("user:123", 100, 3600)
    print("Request allowed")
except RateLimitExceeded as e:
    print(f"Rate limited. Retry in {e.retry_after}s")
```

### Node.js Client

```typescript
import { RateWatch, RateLimitExceededError } from '@ratewatch/client';

const client = new RateWatch('your-key');

try {
    const result = await client.checkWithExceptions('user:123', 100, 3600);
    console.log('Request allowed');
} catch (error) {
    if (error instanceof RateLimitExceededError) {
        console.log(`Rate limited. Retry in ${error.retryAfter}s`);
    }
}
```

## Security

- **TLS 1.3** encryption in production
- **API key authentication** with Blake3 hashing
- **Security headers** (HSTS, X-Frame-Options, etc.)
- **Input validation** and sanitization
- **No PII storage** - only rate limit counters

## Monitoring

- **Prometheus metrics** at `/metrics`
- **Health checks** at `/health` and `/health/detailed`
- **Structured logging** with correlation IDs
- **Real-time analytics** and alerting

## License

Licensed under either of:
- Apache License, Version 2.0
- MIT License

at your option.