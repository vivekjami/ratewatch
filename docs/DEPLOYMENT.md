# RateWatch Deployment Guide

This guide covers various deployment options for RateWatch in production environments.

## Quick Deployment Options

### 1. Docker Compose (Recommended for Small-Medium Scale)

```bash
# Clone the repository
git clone https://github.com/ratewatch/ratewatch.git
cd ratewatch

# Copy environment configuration
cp .env.example .env
# Edit .env with your configuration

# Start the services
docker-compose -f docker-compose.prod.yml up -d
```

### 2. Kubernetes (Recommended for Large Scale)

```bash
# Deploy to Kubernetes
./scripts/deploy.sh production

# Or manually:
kubectl apply -f deploy/k8s/
```

### 3. Binary Deployment

```bash
# Download the latest binary
wget https://github.com/ratewatch/ratewatch/releases/latest/download/ratewatch-linux-x64

# Make executable
chmod +x ratewatch-linux-x64

# Run with environment variables
REDIS_URL=redis://localhost:6379 \
API_KEY_SECRET=your-secret-key \
./ratewatch-linux-x64
```

## Production Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `PORT` | Server port | `8081` | No |
| `REDIS_URL` | Redis connection URL | `redis://127.0.0.1:6379` | Yes |
| `API_KEY_SECRET` | Secret for API key validation | - | Yes |
| `RUST_LOG` | Log level | `info` | No |
| `CORS_ALLOWED_ORIGINS` | CORS origins | `*` | No |
| `DATA_RETENTION_DAYS` | GDPR data retention | `30` | No |

### Security Configuration

```bash
# Generate a secure API key secret
openssl rand -hex 32

# Set in environment
export API_KEY_SECRET="your-generated-secret"

# For production, use specific CORS origins
export CORS_ALLOWED_ORIGINS="https://yourdomain.com,https://api.yourdomain.com"
```

## Infrastructure Requirements

### Minimum Requirements

- **CPU**: 1 vCPU
- **Memory**: 128MB RAM
- **Storage**: 100MB for binary + logs
- **Redis**: 6.0+ (can be shared)

### Recommended Production

- **CPU**: 2+ vCPUs
- **Memory**: 512MB RAM
- **Storage**: 1GB for logs and metrics
- **Redis**: Dedicated instance with persistence

### High Availability Setup

```yaml
# Kubernetes example with 3 replicas
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ratewatch
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 1
      maxSurge: 1
```

## Load Balancing

### Nginx Configuration

```nginx
upstream ratewatch {
    server ratewatch-1:8081;
    server ratewatch-2:8081;
    server ratewatch-3:8081;
}

server {
    listen 80;
    server_name api.yourdomain.com;
    
    location / {
        proxy_pass http://ratewatch;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        
        # Rate limiting at nginx level (optional)
        limit_req zone=api burst=20 nodelay;
    }
}
```

### AWS Application Load Balancer

```yaml
# ALB Target Group
TargetGroup:
  Type: AWS::ElasticLoadBalancingV2::TargetGroup
  Properties:
    Port: 8081
    Protocol: HTTP
    HealthCheckPath: /health
    HealthCheckIntervalSeconds: 30
    HealthyThresholdCount: 2
    UnhealthyThresholdCount: 3
```

## Monitoring Setup

### Prometheus Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'ratewatch'
    static_configs:
      - targets: ['ratewatch:8081']
    metrics_path: /metrics
    scrape_interval: 30s
```

### Grafana Dashboard

Import the dashboard from `monitoring/grafana/dashboards/ratewatch-dashboard.json`

### Alerting Rules

```yaml
# alerts.yml
groups:
  - name: ratewatch
    rules:
      - alert: RateWatchDown
        expr: up{job="ratewatch"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "RateWatch instance is down"
          
      - alert: HighResponseTime
        expr: histogram_quantile(0.95, rate(ratewatch_request_duration_seconds_bucket[5m])) > 0.5
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "RateWatch response time is high"
```

## Backup and Recovery

### Redis Backup

```bash
# Enable Redis persistence
redis-server --appendonly yes --save 900 1

# Backup Redis data
redis-cli BGSAVE

# Copy backup files
cp /var/lib/redis/dump.rdb /backup/redis-$(date +%Y%m%d).rdb
```

### Configuration Backup

```bash
# Backup environment configuration
cp .env /backup/ratewatch-config-$(date +%Y%m%d).env

# Backup Kubernetes manifests
kubectl get all -n ratewatch -o yaml > /backup/k8s-backup-$(date +%Y%m%d).yaml
```

## Scaling Strategies

### Horizontal Scaling

RateWatch is stateless and can be scaled horizontally:

```bash
# Docker Compose
docker-compose -f docker-compose.prod.yml up -d --scale ratewatch=3

# Kubernetes
kubectl scale deployment ratewatch --replicas=5 -n ratewatch
```

### Vertical Scaling

Increase resources for higher throughput:

```yaml
resources:
  requests:
    memory: "256Mi"
    cpu: "200m"
  limits:
    memory: "512Mi"
    cpu: "1000m"
```

## Security Hardening

### Network Security

```bash
# Firewall rules (iptables example)
iptables -A INPUT -p tcp --dport 8081 -s trusted_network -j ACCEPT
iptables -A INPUT -p tcp --dport 8081 -j DROP

# Redis security
redis-cli CONFIG SET requirepass "strong-password"
```

### Container Security

```dockerfile
# Use non-root user
USER 65534:65534

# Read-only filesystem
--read-only --tmpfs /tmp

# Drop capabilities
--cap-drop=ALL
```

## Troubleshooting

### Common Issues

1. **Connection Refused**
   ```bash
   # Check if service is running
   curl http://localhost:8081/health
   
   # Check logs
   docker logs ratewatch-container
   ```

2. **Redis Connection Failed**
   ```bash
   # Test Redis connectivity
   redis-cli -h redis-host ping
   
   # Check Redis logs
   redis-cli info replication
   ```

3. **High Memory Usage**
   ```bash
   # Check Redis memory usage
   redis-cli info memory
   
   # Monitor RateWatch metrics
   curl http://localhost:8081/metrics | grep memory
   ```

### Performance Tuning

```bash
# Redis optimization
redis-cli CONFIG SET maxmemory-policy allkeys-lru
redis-cli CONFIG SET maxmemory 256mb

# OS-level tuning
echo 'net.core.somaxconn = 65535' >> /etc/sysctl.conf
echo 'vm.overcommit_memory = 1' >> /etc/sysctl.conf
sysctl -p
```

## Migration Guide

### From v0.x to v1.0

1. **Backup existing data**
2. **Update configuration format**
3. **Deploy new version**
4. **Verify functionality**
5. **Update client libraries**

### Zero-Downtime Deployment

```bash
# Rolling update with Kubernetes
kubectl set image deployment/ratewatch ratewatch=ratewatch:v1.0.1 -n ratewatch

# Blue-green deployment
kubectl apply -f deploy/k8s/deployment-green.yaml
# Test green deployment
kubectl patch service ratewatch-service -p '{"spec":{"selector":{"version":"green"}}}'
```

## Support

For deployment issues:
- Check the [troubleshooting guide](TROUBLESHOOTING.md)
- Review [GitHub Issues](https://github.com/ratewatch/ratewatch/issues)
- Join our [Discord community](https://discord.gg/ratewatch)