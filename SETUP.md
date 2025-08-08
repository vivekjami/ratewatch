# 🚀 RateWatch - Complete Setup & Deployment Guide

## Phase 4 Complete - Enterprise Rate Limiting with Analytics Dashboard

### ✅ What's Included

**Phase 1 - Core Rate Limiting:**
- High-performance Rust server with Axum framework
- Redis-backed sliding window rate limiting
- RESTful API with JSON responses
- Health monitoring endpoints

**Phase 2 - Security & GDPR:**
- API key authentication with Blake3 hashing
- GDPR compliance endpoints (data deletion, summaries)
- Security headers (OWASP recommended)
- Request/response logging

**Phase 3 - Client Libraries:**
- Python client library (pip installable)
- Node.js/TypeScript client library (npm publishable)
- Comprehensive documentation and examples

**Phase 4 - Analytics Dashboard:**
- Real-time analytics dashboard with Chart.js
- Request metrics and performance monitoring
- Top API keys and activity logging
- Responsive web interface

---

## 🛠️ Development Setup

### Prerequisites

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Redis
# Ubuntu/Debian:
sudo apt-get install redis-server

# macOS:
brew install redis

# Or use Docker:
docker run -d -p 6379:6379 redis:7-alpine
```

### Quick Start

1. **Clone and Setup:**
```bash
git clone <your-repo>
cd ratewatch
cp .env.example .env
```

2. **Configure Environment:**
```bash
# Edit .env file
API_KEY_SECRET=your-super-secret-key-at-least-32-characters-long
REDIS_URL=redis://127.0.0.1:6379
PORT=8081
```

3. **Generate API Key:**
```bash
# Create your first API key
echo "rw_$(date +%s)_$(openssl rand -hex 16)" > api_key.txt
echo "Your API key: $(cat api_key.txt)"
```

4. **Start Services:**
```bash
# Start Redis
redis-server --daemonize yes

# Build and run RateWatch
cargo build --release
cargo run

# Or in development mode
cargo run
```

5. **Test Installation:**
```bash
./test.sh
```

6. **Access Dashboard:**
   - Open http://localhost:8081/dashboard
   - View real-time analytics and metrics

---

## 🐳 Docker Deployment

### Using Docker Compose (Recommended)

```bash
# Start complete stack
docker-compose up -d

# View logs
docker-compose logs -f

# Stop stack
docker-compose down
```

### Custom Docker Setup

```bash
# Build image
docker build -t ratewatch .

# Run with Redis
docker run -d --name redis redis:7-alpine
docker run -d --name ratewatch \
  --link redis:redis \
  -p 8081:8081 \
  -e REDIS_URL=redis://redis:6379 \
  -e API_KEY_SECRET=your-secret-key \
  ratewatch
```

---

## 🌐 Production Deployment

### 1. Server Setup (Ubuntu/Debian)

```bash
# Create dedicated user
sudo useradd -r -s /bin/false ratewatch
sudo mkdir -p /opt/ratewatch
sudo chown ratewatch:ratewatch /opt/ratewatch

# Install dependencies
sudo apt-get update
sudo apt-get install -y redis-server nginx certbot python3-certbot-nginx

# Build production binary
cargo build --release
sudo cp target/release/ratewatch /opt/ratewatch/
sudo cp -r static /opt/ratewatch/
sudo chown -R ratewatch:ratewatch /opt/ratewatch
```

### 2. SystemD Service

```bash
# Copy service file
sudo cp deploy/ratewatch.service /etc/systemd/system/

# Edit configuration
sudo systemctl edit ratewatch.service
# Add your environment variables

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable ratewatch
sudo systemctl start ratewatch
sudo systemctl status ratewatch
```

### 3. Nginx Reverse Proxy

```bash
# Copy nginx configuration
sudo cp deploy/nginx.conf /etc/nginx/sites-available/ratewatch
sudo ln -s /etc/nginx/sites-available/ratewatch /etc/nginx/sites-enabled/

# Update domain name in config
sudo nano /etc/nginx/sites-available/ratewatch

# Test and reload
sudo nginx -t
sudo systemctl reload nginx
```

### 4. SSL Certificate (Let's Encrypt)

```bash
# Get certificate
sudo certbot --nginx -d your-domain.com

# Test renewal
sudo certbot renew --dry-run
```

---

## ☁️ Cloud Deployment

### AWS ECS/Fargate

1. **Build and push Docker image:**
```bash
# Build for production
docker build -t ratewatch:latest .

# Tag for ECR
docker tag ratewatch:latest your-account.dkr.ecr.region.amazonaws.com/ratewatch:latest

# Push to ECR
docker push your-account.dkr.ecr.region.amazonaws.com/ratewatch:latest
```

2. **Create ECS task definition:**
```json
{
  "family": "ratewatch",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "256",
  "memory": "512",
  "executionRoleArn": "arn:aws:iam::account:role/ecsTaskExecutionRole",
  "containerDefinitions": [
    {
      "name": "ratewatch",
      "image": "your-account.dkr.ecr.region.amazonaws.com/ratewatch:latest",
      "portMappings": [{"containerPort": 8081}],
      "environment": [
        {"name": "REDIS_URL", "value": "redis://your-elasticache-endpoint:6379"},
        {"name": "API_KEY_SECRET", "value": "your-secret-key"}
      ]
    }
  ]
}
```

### DigitalOcean App Platform

```yaml
# .do/app.yaml
name: ratewatch
services:
- name: api
  source_dir: /
  github:
    repo: your-username/ratewatch
    branch: main
  run_command: ./target/release/ratewatch
  environment_slug: ubuntu-22-04
  instance_count: 1
  instance_size_slug: basic-xxs
  http_port: 8081
  envs:
  - key: API_KEY_SECRET
    value: your-secret-key
  - key: REDIS_URL
    value: redis://redis:6379

databases:
- name: redis
  engine: REDIS
  version: "7"
```

### Heroku

```bash
# Create app
heroku create your-ratewatch-app

# Add Redis
heroku addons:create heroku-redis:mini

# Set environment
heroku config:set API_KEY_SECRET=your-secret-key

# Deploy
git push heroku main
```

---

## 🔧 Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PORT` | No | `8081` | Server port |
| `REDIS_URL` | No | `redis://127.0.0.1:6379` | Redis connection |
| `API_KEY_SECRET` | **Yes** | - | Secret for API key validation |
| `RUST_LOG` | No | `info` | Log level (error/warn/info/debug) |

### Rate Limiting Configuration

```rust
// Default rate limiting rules
{
    "key": "user-123",           // Unique identifier
    "limit": 1000,               // Requests per window
    "window": 3600,              // Window in seconds (1 hour)
    "cost": 1                    // Cost per request
}
```

---

## 📊 API Usage

### Authentication

All API endpoints require an API key in the Authorization header:

```bash
curl -H "Authorization: Bearer your-api-key" \
  http://localhost:8081/v1/check
```

### Rate Limiting

```bash
# Check rate limit
curl -X POST http://localhost:8081/v1/check \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "key": "user-123",
    "limit": 100,
    "window": 3600,
    "cost": 1
  }'

# Response
{
  "allowed": true,
  "remaining": 99,
  "reset_in": 3599,
  "retry_after": null
}
```

### Analytics

```bash
# Get statistics
curl -H "Authorization: Bearer your-api-key" \
  http://localhost:8081/v1/analytics/stats

# Get top API keys
curl -H "Authorization: Bearer your-api-key" \
  http://localhost:8081/v1/analytics/top-keys

# Get activity logs
curl -H "Authorization: Bearer your-api-key" \
  http://localhost:8081/v1/analytics/activity
```

### GDPR Compliance

```bash
# Delete user data
curl -X POST http://localhost:8081/v1/privacy/delete \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "user-123",
    "reason": "User requested deletion"
  }'

# Get data summary
curl -X POST http://localhost:8081/v1/privacy/summary \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"user_id": "user-123"}'
```

---

## 📈 Monitoring & Maintenance

### Health Checks

```bash
# Basic health
curl http://localhost:8081/health

# Detailed health (with Redis status)
curl http://localhost:8081/health/detailed
```

### Logs

```bash
# View application logs
sudo journalctl -u ratewatch -f

# View nginx logs
sudo tail -f /var/log/nginx/ratewatch_access.log
sudo tail -f /var/log/nginx/ratewatch_error.log
```

### Performance Monitoring

- **Dashboard:** http://your-domain.com/dashboard
- **Metrics:** Monitor request rates, error rates, response times
- **Redis:** Monitor memory usage and connection count

### Backup & Recovery

```bash
# Backup Redis data
redis-cli --rdb /backup/redis-backup.rdb

# Restore Redis data
sudo systemctl stop redis
sudo cp /backup/redis-backup.rdb /var/lib/redis/dump.rdb
sudo chown redis:redis /var/lib/redis/dump.rdb
sudo systemctl start redis
```

---

## 🛡️ Security Best Practices

1. **API Keys:**
   - Use strong, random API keys (32+ characters)
   - Rotate keys regularly
   - Store securely (environment variables, not code)

2. **Network Security:**
   - Use HTTPS in production (SSL/TLS certificates)
   - Configure firewall rules
   - Limit Redis access to localhost or VPN

3. **Rate Limiting:**
   - Set appropriate limits for your use case
   - Monitor for abuse patterns
   - Implement progressive penalties for violators

4. **Monitoring:**
   - Set up alerts for high error rates
   - Monitor resource usage (CPU, memory, Redis)
   - Log suspicious activity

---

## 🚀 Performance Optimization

### Scaling Strategies

1. **Vertical Scaling:**
   - Increase CPU/memory on single server
   - Optimize Redis configuration
   - Use Redis clustering for large datasets

2. **Horizontal Scaling:**
   - Run multiple RateWatch instances behind load balancer
   - Use Redis Cluster for distributed storage
   - Implement consistent hashing for key distribution

3. **Caching:**
   - Redis is already used for caching
   - Consider Redis optimization (maxmemory policies)
   - Monitor cache hit ratios

### Redis Optimization

```bash
# Add to redis.conf
maxmemory 2gb
maxmemory-policy allkeys-lru
tcp-keepalive 60
save 900 1
save 300 10
save 60 10000
```

---

## ✅ Testing

### Automated Testing

```bash
# Run unit tests
cargo test

# Run integration tests
./test.sh

# Load testing with different scenarios
./test.sh http://localhost:8081 your-api-key
```

### Manual Testing

1. **Basic Functionality:**
   - Health endpoints
   - Rate limiting API
   - Dashboard access

2. **Security Testing:**
   - API key validation
   - Invalid request handling
   - CORS configuration

3. **Performance Testing:**
   - Load testing with artillery/wrk
   - Memory usage monitoring
   - Response time analysis

---

## 🎉 Success! RateWatch Phase 4 Complete

You now have a fully-featured, enterprise-grade rate limiting service with:

- ✅ High-performance rate limiting
- ✅ Security & GDPR compliance  
- ✅ Client libraries (Python & Node.js)
- ✅ Real-time analytics dashboard
- ✅ Production deployment guides
- ✅ Monitoring & maintenance tools

**Quick Links:**
- 📊 Dashboard: http://localhost:8081/dashboard
- ❤️ Health: http://localhost:8081/health
- 📋 Test: `./test.sh`

**Next Steps:**
1. Deploy to your production environment
2. Configure monitoring and alerts
3. Set up client libraries in your applications
4. Scale according to your needs

**Support:**
- Check logs: `sudo journalctl -u ratewatch -f`
- Run tests: `./test.sh`
- Monitor dashboard: `/dashboard`
