# RateWatch Python Client

A Python client library for interacting with the RateWatch API rate limiting service.

## Installation

```bash
pip install ratewatch
```

## Quick Start

```python
from ratewatch import RateWatch

# Initialize the client
client = RateWatch(api_key="your-api-key", base_url="http://localhost:8081")

# Check rate limit
result = client.check(
    key="user:123",
    limit=100,
    window=3600,  # 1 hour in seconds
    cost=1
)

if result.allowed:
    print(f"Request allowed. {result.remaining} requests remaining.")
else:
    print(f"Rate limit exceeded. Try again in {result.retry_after} seconds.")
```

## Features

- **Rate Limit Checking**: Check if a request is allowed based on configurable limits
- **GDPR Compliance**: Delete user data and get data summaries
- **Health Monitoring**: Check service health and dependencies
- **Error Handling**: Comprehensive exception handling for different error scenarios
- **Type Safety**: Full type hints for better IDE support

## API Reference

### RateWatch Class

#### `__init__(api_key: str, base_url: str = "http://localhost:8081")`

Initialize the RateWatch client.

- `api_key`: Your API key for authentication
- `base_url`: The base URL of the RateWatch service

#### `check(key: str, limit: int, window: int, cost: int = 1) -> RateLimitResult`

Check rate limit for a given key.

- `key`: Unique identifier for the rate limit (e.g., "user:123", "api:endpoint")
- `limit`: Maximum number of requests allowed in the window
- `window`: Time window in seconds
- `cost`: Cost of this request (default: 1)

Returns a `RateLimitResult` object with:
- `allowed`: Whether the request is allowed
- `remaining`: Number of requests remaining in the window
- `reset_in`: Seconds until the window resets
- `retry_after`: Seconds to wait before retrying (if not allowed)

#### `delete_user_data(user_id: str, reason: str = "user_request") -> bool`

Delete all data for a user (GDPR compliance).

- `user_id`: The user ID to delete data for
- `reason`: Reason for deletion (default: "user_request")

Returns `True` if successful, `False` otherwise.

#### `get_user_data_summary(user_id: str) -> Dict`

Get summary of user data (GDPR compliance).

- `user_id`: The user ID to get data summary for

Returns a dictionary with user data summary.

#### `health_check() -> Dict`

Check service health.

Returns a dictionary with health status.

#### `detailed_health_check() -> Dict`

Get detailed health information including dependencies.

Returns a dictionary with detailed health information.

### RateWatchClient Class (Enhanced)

Extends `RateWatch` with additional error handling:

#### `check_with_exceptions(key: str, limit: int, window: int, cost: int = 1) -> RateLimitResult`

Same as `check()` but raises exceptions for error cases:
- `RateLimitExceeded`: When rate limit is exceeded
- `AuthenticationError`: When API key is invalid
- `RateWatchError`: For other errors

## Exception Classes

- `RateWatchError`: Base exception for all client errors
- `RateLimitExceeded`: Raised when rate limit is exceeded
- `AuthenticationError`: Raised when API key authentication fails

## Examples

### Basic Rate Limiting

```python
from ratewatch import RateWatch

client = RateWatch(api_key="your-api-key")

# Check if user can make a request
result = client.check(
    key="user:alice",
    limit=10,
    window=60  # 10 requests per minute
)

if result.allowed:
    # Process the request
    print("Request allowed")
else:
    # Return rate limit error
    print(f"Rate limited. Try again in {result.retry_after} seconds")
```

### With Exception Handling

```python
from ratewatch import RateWatchClient, RateLimitExceeded, AuthenticationError

client = RateWatchClient(api_key="your-api-key")

try:
    result = client.check_with_exceptions(
        key="api:upload",
        limit=5,
        window=3600,  # 5 uploads per hour
        cost=1
    )
    print(f"Upload allowed. {result.remaining} uploads remaining.")
except RateLimitExceeded as e:
    print(f"Upload rate limit exceeded. Retry in {e.retry_after} seconds")
except AuthenticationError:
    print("Invalid API key")
```

### GDPR Compliance

```python
# Delete user data
success = client.delete_user_data("user:123", reason="account_deletion")
if success:
    print("User data deleted successfully")

# Get user data summary
summary = client.get_user_data_summary("user:123")
print(f"User has {summary['keys_count']} rate limit keys")
```

### Health Monitoring

```python
# Basic health check
health = client.health_check()
print(f"Service status: {health['status']}")

# Detailed health check
detailed = client.detailed_health_check()
print(f"Redis status: {detailed['dependencies']['redis']['status']}")
```

## Development

### Setup

```bash
git clone https://github.com/ratewatch/client-python
cd client-python
pip install -e .[dev]
```

### Testing

```bash
pytest
```

### Code Formatting

```bash
black .
flake8 .
mypy .
```

## License

MIT License
