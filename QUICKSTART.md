# RateWatch Quickstart Guide

This document provides a comprehensive guide to deploying and using RateWatch without spending any money on licenses, API services, or commercial tools.

## 🚀 Quick Deployment

### Option 1: Local Deployment (5 minutes)

```bash
# Clone the repo
git clone https://github.com/yourusername/ratewatch.git
cd ratewatch

# Start local deployment
./deploy.sh --local

# Generate API key
./scripts/generate_api_key.sh client1

# Your API key will be saved to api_key_client1.txt
```

### Option 2: Free Cloud Deployment (Oracle Cloud Free Tier)

```bash
# 1. Sign up for Oracle Cloud Free Tier (no credit card required)
# 2. Create Always Free VM with Ubuntu
# 3. SSH into your VM
# 4. Clone the repo
git clone https://github.com/yourusername/ratewatch.git
cd ratewatch

# 5. Deploy
./deploy.sh yourdomain.com your@email.com

# 6. Generate API key
./scripts/generate_api_key.sh client1
```

## 🔑 Using RateWatch API

### Checking Rate Limits

```bash
# Using curl (Replace API_KEY with your key from api_key.txt)
curl -X POST http://localhost:8081/v1/check \
  -H "Authorization: Bearer API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "key": "user:123",
    "limit": 100,
    "window": 3600,
    "cost": 1
  }'
```

### Python Client

```python
from ratewatch import RateWatch

# Initialize with your API key
limiter = RateWatch("YOUR_API_KEY", base_url="http://localhost:8081")

# Check rate limit
result = limiter.check(
    key="user:123",
    limit=100,
    window=3600
)

if result.allowed:
    print(f"Request allowed! Remaining: {result.remaining}")
else:
    print(f"Rate limited! Try again in {result.retry_after} seconds")
```

### Node.js Client

```javascript
import { RateWatch } from '@ratewatch/client';

// Initialize with your API key
const limiter = new RateWatch("YOUR_API_KEY", "http://localhost:8081");

// Check rate limit
const result = await limiter.check({
  key: "user:123",
  limit: 100,
  window: 3600
});

if (result.allowed) {
  console.log(`Request allowed! Remaining: ${result.remaining}`);
} else {
  console.log(`Rate limited! Try again in ${result.retryAfter} seconds`);
}
```

## 📊 Accessing the Dashboard

The analytics dashboard is available at:
- http://localhost:8081/dashboard

No login required - it shows real-time metrics for your API usage.

## 🔧 Configuration Options

Edit the `.env` file to customize RateWatch:

```bash
# Server port (default: 8081)
PORT=8081

# Redis connection (required)
REDIS_URL=redis://127.0.0.1:6379

# Secret for API key validation
API_KEY_SECRET=your-secure-secret
```

## 📈 Monitoring

RateWatch includes built-in monitoring:

```bash
# Start the monitoring stack
cd monitoring
docker-compose up -d

# Access Grafana at http://localhost:3000
# Default login: admin/admin
```

## 🛠️ Common Tasks

### Generating API Keys

```bash
# Generate a new API key
./scripts/generate_api_key.sh client_name
```

### Testing the Server

```bash
# Run validation script
./validate.sh
```

### Getting Help

For additional help, refer to:
- `SETUP.md` - Detailed setup instructions
- `FREE_DEPLOYMENT_GUIDE.md` - Free deployment options
- `README.md` - Project overview

## 📝 No License/Service Fees

RateWatch is 100% free to use. There are:
- NO paid API services required
- NO commercial license fees
- NO paid dependencies
- NO usage limits

This is a self-hosted solution that can be deployed on completely free infrastructure.

Enjoy your enterprise-grade rate limiting service! 🚀
