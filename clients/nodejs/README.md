# RateWatch Node.js Client

A TypeScript/Node.js client library for interacting with the RateWatch API rate limiting service.

## Installation

```bash
npm install @ratewatch/client
# or
yarn add @ratewatch/client
```

## Quick Start

```typescript
import { RateWatch } from '@ratewatch/client';

// Initialize the client
const client = new RateWatch('your-api-key', 'http://localhost:8081');

// Check rate limit
const result = await client.check(
  'user:123',
  100,
  3600, // 1 hour in seconds
  1
);

if (result.allowed) {
  console.log(`Request allowed. ${result.remaining} requests remaining.`);
} else {
  console.log(`Rate limit exceeded. Try again in ${result.retryAfter} seconds.`);
}
```

## Features

- **TypeScript Support**: Full type definitions included
- **Rate Limit Checking**: Check if a request is allowed based on configurable limits
- **GDPR Compliance**: Delete user data and get data summaries
- **Health Monitoring**: Check service health and dependencies
- **Error Handling**: Comprehensive exception handling for different error scenarios
- **Async/Await**: Modern Promise-based API

## API Reference

### RateWatch Class

#### `constructor(apiKey: string, baseUrl?: string)`

Initialize the RateWatch client.

- `apiKey`: Your API key for authentication
- `baseUrl`: The base URL of the RateWatch service (default: "http://localhost:8081")

#### `check(key: string, limit: number, window: number, cost?: number): Promise<RateLimitResult>`

Check rate limit for a given key.

- `key`: Unique identifier for the rate limit (e.g., "user:123", "api:endpoint")
- `limit`: Maximum number of requests allowed in the window
- `window`: Time window in seconds
- `cost`: Cost of this request (default: 1)

Returns a `RateLimitResult` object with:
- `allowed`: Whether the request is allowed
- `remaining`: Number of requests remaining in the window
- `resetIn`: Seconds until the window resets
- `retryAfter`: Seconds to wait before retrying (if not allowed)

#### `deleteUserData(userId: string, reason?: string): Promise<boolean>`

Delete all data for a user (GDPR compliance).

- `userId`: The user ID to delete data for
- `reason`: Reason for deletion (default: "user_request")

Returns `true` if successful, `false` otherwise.

#### `getUserDataSummary(userId: string): Promise<UserDataSummary>`

Get summary of user data (GDPR compliance).

- `userId`: The user ID to get data summary for

Returns a `UserDataSummary` object with user data information.

#### `healthCheck(): Promise<HealthStatus>`

Check service health.

Returns a `HealthStatus` object with health information.

#### `detailedHealthCheck(): Promise<HealthStatus>`

Get detailed health information including dependencies.

Returns a `HealthStatus` object with detailed health information.

#### `checkWithExceptions(key: string, limit: number, window: number, cost?: number): Promise<RateLimitResult>`

Same as `check()` but throws exceptions for error cases:
- `RateLimitExceededError`: When rate limit is exceeded
- `AuthenticationError`: When API key is invalid
- `RateWatchError`: For other errors

## Types

### RateLimitResult

```typescript
interface RateLimitResult {
  allowed: boolean;
  remaining: number;
  resetIn: number;
  retryAfter?: number;
}
```

### UserDataSummary

```typescript
interface UserDataSummary {
  user_id: string;
  keys_count: number;
  total_requests: number;
  data_types: string[];
}
```

### HealthStatus

```typescript
interface HealthStatus {
  status: string;
  timestamp: string;
  dependencies?: {
    redis: {
      status: string;
      latency_ms?: number;
    };
  };
}
```

## Exception Classes

- `RateWatchError`: Base exception for all client errors
- `RateLimitExceededError`: Raised when rate limit is exceeded
- `AuthenticationError`: Raised when API key authentication fails

## Examples

### Basic Rate Limiting

```typescript
import { RateWatch } from '@ratewatch/client';

const client = new RateWatch('your-api-key');

// Check if user can make a request
try {
  const result = await client.check(
    'user:alice',
    10,
    60 // 10 requests per minute
  );

  if (result.allowed) {
    // Process the request
    console.log('Request allowed');
  } else {
    // Return rate limit error
    console.log(`Rate limited. Try again in ${result.retryAfter} seconds`);
  }
} catch (error) {
  console.error('Error checking rate limit:', error);
}
```

### With Exception Handling

```typescript
import { 
  RateWatch, 
  RateLimitExceededError, 
  AuthenticationError 
} from '@ratewatch/client';

const client = new RateWatch('your-api-key');

try {
  const result = await client.checkWithExceptions(
    'api:upload',
    5,
    3600, // 5 uploads per hour
    1
  );
  console.log(`Upload allowed. ${result.remaining} uploads remaining.`);
} catch (error) {
  if (error instanceof RateLimitExceededError) {
    console.log(`Upload rate limit exceeded. Retry in ${error.retryAfter} seconds`);
  } else if (error instanceof AuthenticationError) {
    console.log('Invalid API key');
  } else {
    console.error('Unexpected error:', error);
  }
}
```

### GDPR Compliance

```typescript
// Delete user data
const success = await client.deleteUserData('user:123', 'account_deletion');
if (success) {
  console.log('User data deleted successfully');
}

// Get user data summary
const summary = await client.getUserDataSummary('user:123');
console.log(`User has ${summary.keys_count} rate limit keys`);
```

### Health Monitoring

```typescript
// Basic health check
const health = await client.healthCheck();
console.log(`Service status: ${health.status}`);

// Detailed health check
const detailed = await client.detailedHealthCheck();
if (detailed.dependencies?.redis) {
  console.log(`Redis status: ${detailed.dependencies.redis.status}`);
}
```

### Express.js Middleware

```typescript
import express from 'express';
import { RateWatch } from '@ratewatch/client';

const app = express();
const rateWatch = new RateWatch('your-api-key');

// Rate limiting middleware
async function rateLimitMiddleware(req: express.Request, res: express.Response, next: express.NextFunction) {
  try {
    const userId = req.user?.id || req.ip; // Use user ID or IP
    const result = await rateWatch.check(`user:${userId}`, 100, 3600); // 100 requests per hour

    if (result.allowed) {
      res.set({
        'X-RateLimit-Limit': '100',
        'X-RateLimit-Remaining': result.remaining.toString(),
        'X-RateLimit-Reset': new Date(Date.now() + result.resetIn * 1000).toISOString()
      });
      next();
    } else {
      res.status(429).json({
        error: 'Rate limit exceeded',
        retryAfter: result.retryAfter
      });
    }
  } catch (error) {
    console.error('Rate limit check failed:', error);
    next(); // Continue on error (fail open)
  }
}

app.use(rateLimitMiddleware);
```

## Development

### Setup

```bash
git clone https://github.com/ratewatch/client-nodejs
cd client-nodejs
npm install
```

### Build

```bash
npm run build
```

### Testing

```bash
npm test
npm run test:coverage
```

### Linting and Formatting

```bash
npm run lint
npm run format
```

## License

MIT License
