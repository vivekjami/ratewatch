# RateWatch Client Examples

This directory contains practical examples of using the RateWatch client libraries in real-world scenarios.

## Quick Start Examples

### Python Example

```python
#!/usr/bin/env python3
"""
Example: Using RateWatch Python client in a web application
"""

from ratewatch import RateWatch, RateLimitExceeded, AuthenticationError
import time

# Initialize the client
client = RateWatch(
    api_key="your-production-api-key-here", 
    base_url="https://your-ratewatch-server.com"
)

def api_endpoint_handler(user_id: str, endpoint: str):
    """Example API endpoint with rate limiting"""
    try:
        # Check rate limit: 100 requests per hour per user
        result = client.check(
            key=f"user:{user_id}:api",
            limit=100,
            window=3600,  # 1 hour
            cost=1
        )
        
        if result.allowed:
            # Process the request
            print(f"Request allowed. {result.remaining} requests remaining.")
            return {"success": True, "data": "API response here"}
        else:
            # Rate limit exceeded
            return {
                "error": "Rate limit exceeded",
                "retry_after": result.retry_after
            }, 429
            
    except AuthenticationError:
        return {"error": "Invalid API configuration"}, 500
    except Exception as e:
        return {"error": f"Rate limiting service unavailable: {e}"}, 503

def handle_user_deletion(user_id: str):
    """GDPR: Delete all user data"""
    try:
        success = client.delete_user_data(user_id, reason="user_request")
        if success:
            print(f"User {user_id} data deleted successfully")
        else:
            print(f"Failed to delete user {user_id} data")
    except Exception as e:
        print(f"Error deleting user data: {e}")

# Example usage
if __name__ == "__main__":
    # Test the rate limiting
    for i in range(5):
        result = api_endpoint_handler("user123", "/api/data")
        print(f"Request {i+1}: {result}")
        time.sleep(1)
```

### Node.js Example

```javascript
/**
 * Example: Using RateWatch Node.js client with Express.js
 */

const express = require('express');
const { RateWatch, RateLimitExceededError, AuthenticationError } = require('@ratewatch/client');

const app = express();
const rateWatch = new RateWatch(
  process.env.RATEWATCH_API_KEY || 'your-production-api-key-here',
  process.env.RATEWATCH_URL || 'https://your-ratewatch-server.com'
);

// Rate limiting middleware
async function rateLimitMiddleware(req, res, next) {
  try {
    const userId = req.user?.id || req.ip;
    const endpoint = req.route?.path || req.path;
    
    const result = await rateWatch.check(
      `user:${userId}:endpoint:${endpoint}`,
      100, // 100 requests
      3600, // per hour
      1
    );

    // Set rate limit headers
    res.set({
      'X-RateLimit-Limit': '100',
      'X-RateLimit-Remaining': result.remaining.toString(),
      'X-RateLimit-Reset': new Date(Date.now() + result.resetIn * 1000).toISOString()
    });

    if (result.allowed) {
      next();
    } else {
      res.status(429).json({
        error: 'Rate limit exceeded',
        retryAfter: result.retryAfter
      });
    }
  } catch (error) {
    if (error instanceof AuthenticationError) {
      console.error('RateWatch authentication failed:', error.message);
    } else {
      console.error('RateWatch error:', error.message);
    }
    // Fail open - allow request if rate limiting service is down
    next();
  }
}

// Apply rate limiting to all routes
app.use(rateLimitMiddleware);

// API endpoints
app.get('/api/data', (req, res) => {
  res.json({ message: 'Hello, World!', timestamp: new Date().toISOString() });
});

// GDPR compliance endpoint
app.delete('/api/users/:userId', async (req, res) => {
  try {
    const { userId } = req.params;
    
    // Delete user data from RateWatch
    const success = await rateWatch.deleteUserData(userId, 'user_request');
    
    if (success) {
      res.json({ message: 'User data deleted successfully' });
    } else {
      res.status(500).json({ error: 'Failed to delete user data' });
    }
  } catch (error) {
    console.error('Error deleting user data:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Health check endpoint
app.get('/health', async (req, res) => {
  try {
    const health = await rateWatch.detailedHealthCheck();
    res.json({
      status: 'ok',
      ratewatch: health
    });
  } catch (error) {
    res.status(503).json({
      status: 'degraded',
      error: 'RateWatch service unavailable'
    });
  }
});

const PORT = process.env.PORT || 3000;
app.listen(PORT, () => {
  console.log(`Server running on port ${PORT}`);
});
```

## Advanced Usage Patterns

### 1. Multi-tier Rate Limiting

```python
# Python example: Different limits for different user tiers
def check_user_rate_limit(user_id: str, user_tier: str):
    limits = {
        'free': {'limit': 10, 'window': 3600},      # 10/hour
        'premium': {'limit': 100, 'window': 3600},   # 100/hour
        'enterprise': {'limit': 1000, 'window': 3600} # 1000/hour
    }
    
    config = limits.get(user_tier, limits['free'])
    
    return client.check(
        key=f"user:{user_id}:tier:{user_tier}",
        limit=config['limit'],
        window=config['window'],
        cost=1
    )
```

```javascript
// Node.js example: Different costs for different operations
async function checkOperationLimit(userId, operation) {
  const operationCosts = {
    'read': 1,
    'write': 5,
    'delete': 10,
    'bulk_export': 50
  };
  
  const cost = operationCosts[operation] || 1;
  
  return await rateWatch.check(
    `user:${userId}:operations`,
    1000, // 1000 units per hour
    3600,
    cost
  );
}
```

### 2. Sliding Window with Burst Allowance

```python
# Python example: Allow bursts but enforce average rate
def check_burst_rate_limit(user_id: str):
    # Short-term burst limit: 10 requests per minute
    burst_result = client.check(
        key=f"user:{user_id}:burst",
        limit=10,
        window=60,
        cost=1
    )
    
    if not burst_result.allowed:
        return burst_result
    
    # Long-term average: 100 requests per hour
    average_result = client.check(
        key=f"user:{user_id}:average",
        limit=100,
        window=3600,
        cost=1
    )
    
    return average_result
```

### 3. API Key-based Rate Limiting

```javascript
// Node.js example: Rate limiting by API key
async function apiKeyRateLimit(apiKey, req, res, next) {
  try {
    // Different limits for different API key types
    const keyType = await getApiKeyType(apiKey);
    const limits = {
      'basic': { limit: 1000, window: 3600 },
      'professional': { limit: 10000, window: 3600 },
      'enterprise': { limit: 100000, window: 3600 }
    };
    
    const config = limits[keyType] || limits['basic'];
    
    const result = await rateWatch.check(
      `apikey:${apiKey}`,
      config.limit,
      config.window,
      1
    );
    
    if (result.allowed) {
      req.rateLimitInfo = result;
      next();
    } else {
      res.status(429).json({
        error: 'API rate limit exceeded',
        limit: config.limit,
        window: config.window,
        retryAfter: result.retryAfter
      });
    }
  } catch (error) {
    // Fail open
    next();
  }
}
```

### 4. Geographic Rate Limiting

```python
# Python example: Different limits by geographic region
def check_geo_rate_limit(user_id: str, country_code: str):
    # Different limits for different regions
    geo_limits = {
        'US': {'limit': 1000, 'window': 3600},
        'EU': {'limit': 1000, 'window': 3600},
        'CN': {'limit': 100, 'window': 3600},   # Stricter limits
        'default': {'limit': 500, 'window': 3600}
    }
    
    config = geo_limits.get(country_code, geo_limits['default'])
    
    return client.check(
        key=f"user:{user_id}:geo:{country_code}",
        limit=config['limit'],
        window=config['window'],
        cost=1
    )
```

## Error Handling Best Practices

### Python Error Handling

```python
from ratewatch import RateWatch, RateLimitExceeded, AuthenticationError, RateWatchError
import logging

logger = logging.getLogger(__name__)

def robust_rate_check(key: str, limit: int, window: int):
    """Rate limiting with comprehensive error handling"""
    try:
        result = client.check_with_exceptions(key, limit, window, 1)
        return {"allowed": True, "remaining": result.remaining}
        
    except RateLimitExceeded as e:
        logger.info(f"Rate limit exceeded for {key}: retry in {e.retry_after}s")
        return {
            "allowed": False, 
            "error": "rate_limit_exceeded",
            "retry_after": e.retry_after
        }
        
    except AuthenticationError as e:
        logger.error(f"Authentication failed: {e}")
        return {"allowed": False, "error": "auth_failed"}
        
    except RateWatchError as e:
        logger.error(f"RateWatch service error: {e}")
        # Fail open - allow request if service is down
        return {"allowed": True, "error": "service_unavailable"}
        
    except Exception as e:
        logger.error(f"Unexpected error: {e}")
        # Fail open
        return {"allowed": True, "error": "unknown"}
```

### Node.js Error Handling

```javascript
const logger = require('winston');

async function robustRateCheck(key, limit, window) {
  try {
    const result = await rateWatch.checkWithExceptions(key, limit, window, 1);
    return { allowed: true, remaining: result.remaining };
    
  } catch (error) {
    if (error instanceof RateLimitExceededError) {
      logger.info(`Rate limit exceeded for ${key}: retry in ${error.retryAfter}s`);
      return {
        allowed: false,
        error: 'rate_limit_exceeded',
        retryAfter: error.retryAfter
      };
    } else if (error instanceof AuthenticationError) {
      logger.error(`Authentication failed: ${error.message}`);
      return { allowed: false, error: 'auth_failed' };
    } else {
      logger.error(`RateWatch error: ${error.message}`);
      // Fail open - allow request if service is down
      return { allowed: true, error: 'service_unavailable' };
    }
  }
}
```

## Monitoring and Observability

### Health Check Integration

```python
# Python: Regular health checks
import schedule
import time

def check_ratewatch_health():
    try:
        health = client.detailed_health_check()
        if health['status'] != 'ok':
            logger.warning(f"RateWatch health degraded: {health}")
        else:
            logger.info("RateWatch is healthy")
    except Exception as e:
        logger.error(f"RateWatch health check failed: {e}")

# Check every 5 minutes
schedule.every(5).minutes.do(check_ratewatch_health)
```

```javascript
// Node.js: Health check with metrics
const prometheus = require('prom-client');

const ratewatchHealthGauge = new prometheus.Gauge({
  name: 'ratewatch_health_status',
  help: 'RateWatch service health status'
});

async function checkRatewatchHealth() {
  try {
    const health = await rateWatch.detailedHealthCheck();
    const isHealthy = health.status === 'ok' ? 1 : 0;
    ratewatchHealthGauge.set(isHealthy);
    
    if (health.dependencies?.redis) {
      console.log(`Redis latency: ${health.dependencies.redis.latency_ms}ms`);
    }
  } catch (error) {
    ratewatchHealthGauge.set(0);
    console.error('RateWatch health check failed:', error.message);
  }
}

// Check every 30 seconds
setInterval(checkRatewatchHealth, 30000);
```

## Testing with RateWatch

### Python Testing

```python
import unittest
from unittest.mock import patch, Mock

class TestRateLimiting(unittest.TestCase):
    def setUp(self):
        self.client = RateWatch("test-key", "http://localhost:8081")
    
    @patch('requests.Session.post')
    def test_rate_limit_allowed(self, mock_post):
        # Mock successful rate limit check
        mock_response = Mock()
        mock_response.json.return_value = {
            "allowed": True,
            "remaining": 99,
            "reset_in": 3600
        }
        mock_response.raise_for_status.return_value = None
        mock_post.return_value = mock_response
        
        result = self.client.check("test:key", 100, 3600, 1)
        
        self.assertTrue(result.allowed)
        self.assertEqual(result.remaining, 99)
```

### Node.js Testing (Jest)

```javascript
const { RateWatch } = require('@ratewatch/client');
const axios = require('axios');

jest.mock('axios');
const mockedAxios = axios;

describe('RateWatch Client', () => {
  let client;
  
  beforeEach(() => {
    client = new RateWatch('test-key', 'http://localhost:8081');
    mockedAxios.create.mockReturnValue(mockedAxios);
  });
  
  test('should handle successful rate limit check', async () => {
    mockedAxios.post.mockResolvedValue({
      data: {
        allowed: true,
        remaining: 99,
        reset_in: 3600
      }
    });
    
    const result = await client.check('test:key', 100, 3600, 1);
    
    expect(result.allowed).toBe(true);
    expect(result.remaining).toBe(99);
  });
});
```

## Production Deployment

### Environment Configuration

```bash
# .env file
RATEWATCH_API_KEY=your-production-api-key-here
RATEWATCH_URL=https://ratewatch.yourcompany.com
RATEWATCH_TIMEOUT=10000
```

### Docker Integration

```dockerfile
# Dockerfile example
FROM node:18-alpine
WORKDIR /app
COPY package*.json ./
RUN npm ci --only=production
COPY . .
ENV RATEWATCH_API_KEY=""
ENV RATEWATCH_URL="http://ratewatch:8081"
EXPOSE 3000
CMD ["node", "server.js"]
```

### Kubernetes Configuration

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: api-server
  template:
    metadata:
      labels:
        app: api-server
    spec:
      containers:
      - name: api-server
        image: your-api:latest
        env:
        - name: RATEWATCH_API_KEY
          valueFrom:
            secretKeyRef:
              name: ratewatch-secrets
              key: api-key
        - name: RATEWATCH_URL
          value: "http://ratewatch-service:8081"
```

This comprehensive example guide shows how to integrate RateWatch client libraries into real-world applications with proper error handling, monitoring, and production deployment patterns.
