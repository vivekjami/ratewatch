#!/bin/bash

# RateWatch SaaS Production Deployment Script
# Complete automation for production deployment

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# Configuration
DOMAIN=${1:-"api.yourdomain.com"}
EMAIL=${2:-"admin@yourdomain.com"}
ENVIRONMENT=${3:-"production"}
REGISTRY="ghcr.io/your-org"
IMAGE_NAME="ratewatch"

echo -e "${BOLD}🚀 RateWatch SaaS Production Deployment${NC}"
echo "=============================================="
echo -e "${BLUE}Domain: $DOMAIN${NC}"
echo -e "${BLUE}Email: $EMAIL${NC}"
echo -e "${BLUE}Environment: $ENVIRONMENT${NC}"
echo

# Function to check command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to check and report status
check_status() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✅ $2${NC}"
    else
        echo -e "${RED}❌ $2${NC}"
        exit 1
    fi
}

# Check prerequisites
echo -e "${BOLD}🔧 Checking prerequisites...${NC}"

command_exists docker
check_status $? "Docker installed"

command_exists docker-compose
check_status $? "Docker Compose installed"

command_exists git
check_status $? "Git installed"

# Build version
VERSION=$(git rev-parse --short HEAD)
echo -e "${BLUE}📦 Building version: $VERSION${NC}"

# Clean up old artifacts
echo -e "${BOLD}🧹 Cleaning up...${NC}"
rm -rf target/
docker system prune -f
check_status $? "Cleanup completed"

# Build optimized release
echo -e "${BOLD}🔨 Building optimized release...${NC}"
cargo build --release
check_status $? "Release build completed"

# Run comprehensive tests
echo -e "${BOLD}🧪 Running comprehensive tests...${NC}"
cargo test --all --verbose
check_status $? "All tests passed"

# Security audit
echo -e "${BOLD}🔐 Security audit...${NC}"
cargo audit
check_status $? "Security audit clean"

# Build Docker image
echo -e "${BOLD}🐳 Building Docker image...${NC}"
docker build -t $IMAGE_NAME:$VERSION .
docker tag $IMAGE_NAME:$VERSION $IMAGE_NAME:latest
check_status $? "Docker image built"

# Test Docker image
echo -e "${BOLD}🧪 Testing Docker image...${NC}"
docker run -d --name redis-test redis:7-alpine
sleep 3
docker run -d --name ratewatch-test --link redis-test:redis -p 8082:8081 \
  -e REDIS_URL=redis://redis-test:6379 \
  -e API_KEY_SECRET=test-secret \
  $IMAGE_NAME:$VERSION

sleep 5
curl -f http://localhost:8082/health
check_status $? "Docker image test passed"

# Cleanup test containers
docker stop ratewatch-test redis-test
docker rm ratewatch-test redis-test

# Generate production configuration
echo -e "${BOLD}📝 Generating production configuration...${NC}"

# Create production environment file
cat > .env.production << EOF
# RateWatch Production Configuration
PORT=8081
RUST_LOG=info
WORKERS=4

# Redis Configuration
REDIS_URL=redis://redis-cluster:6379
REDIS_POOL_SIZE=20
REDIS_TIMEOUT=5000

# Security
API_KEY_SECRET=$(openssl rand -hex 32)
CORS_ORIGINS=https://$DOMAIN

# Monitoring
METRICS_ENABLED=true
HEALTH_CHECK_INTERVAL=30

# Rate Limiting Defaults
DEFAULT_RATE_LIMIT=1000
DEFAULT_WINDOW=3600
MAX_BURST_SIZE=100
EOF

# Generate secure API key for testing
API_KEY="rw_$(date +%s)_$(openssl rand -hex 16)"
echo "$API_KEY" > production_api_key.txt
chmod 600 production_api_key.txt

echo -e "${GREEN}✅ Production API key generated: production_api_key.txt${NC}"

# Create production docker-compose
cat > docker-compose.production.yml << 'EOF'
version: '3.8'

services:
  ratewatch:
    image: ratewatch:latest
    deploy:
      replicas: 3
      resources:
        limits:
          cpus: '2.0'
          memory: 4G
        reservations:
          cpus: '1.0'
          memory: 2G
      restart_policy:
        condition: on-failure
        delay: 5s
        max_attempts: 3
    environment:
      - REDIS_URL=redis://redis-cluster:6379
      - API_KEY_SECRET=${API_KEY_SECRET}
      - RUST_LOG=info
      - PORT=8081
    ports:
      - "8081:8081"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s
    depends_on:
      - redis-cluster
    networks:
      - ratewatch-network

  redis-cluster:
    image: redis:7-alpine
    command: >
      redis-server 
      --appendonly yes 
      --maxmemory 8gb
      --maxmemory-policy allkeys-lru
    volumes:
      - redis_data:/data
    ports:
      - "6379:6379"
    networks:
      - ratewatch-network

  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx/nginx.prod.conf:/etc/nginx/nginx.conf
      - ./ssl:/etc/ssl/certs
    depends_on:
      - ratewatch
    networks:
      - ratewatch-network

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'
    networks:
      - ratewatch-network

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin123
    volumes:
      - grafana_data:/var/lib/grafana
    networks:
      - ratewatch-network

volumes:
  redis_data:
  prometheus_data:
  grafana_data:

networks:
  ratewatch-network:
    driver: bridge
EOF

# Create nginx production config
mkdir -p nginx
cat > nginx/nginx.prod.conf << EOF
events {
    worker_connections 1024;
}

http {
    upstream ratewatch_backend {
        server ratewatch:8081;
    }
    
    server {
        listen 80;
        server_name $DOMAIN;
        return 301 https://\$server_name\$request_uri;
    }
    
    server {
        listen 443 ssl;
        server_name $DOMAIN;
        
        ssl_certificate /etc/ssl/certs/fullchain.pem;
        ssl_certificate_key /etc/ssl/certs/privkey.pem;
        
        add_header Strict-Transport-Security "max-age=31536000" always;
        add_header X-Frame-Options DENY always;
        add_header X-Content-Type-Options nosniff always;
        
        location / {
            proxy_pass http://ratewatch_backend;
            proxy_set_header Host \$host;
            proxy_set_header X-Real-IP \$remote_addr;
            proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto \$scheme;
        }
    }
}
EOF

# SSL setup
echo -e "${BOLD}🔒 Setting up SSL certificates...${NC}"
mkdir -p ssl

if command_exists certbot; then
    echo "Using certbot for SSL certificates..."
    certbot certonly --standalone --email $EMAIL --agree-tos --no-eff-email -d $DOMAIN
    cp /etc/letsencrypt/live/$DOMAIN/fullchain.pem ssl/
    cp /etc/letsencrypt/live/$DOMAIN/privkey.pem ssl/
else
    echo "Creating self-signed certificates for testing..."
    openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
        -keyout ssl/privkey.pem \
        -out ssl/fullchain.pem \
        -subj "/C=US/ST=State/L=City/O=Organization/CN=$DOMAIN"
fi

check_status $? "SSL certificates configured"

# Deploy services
echo -e "${BOLD}🚀 Deploying services...${NC}"
docker-compose -f docker-compose.production.yml up -d
check_status $? "Services deployed"

# Wait for services to be ready
echo -e "${BOLD}⏳ Waiting for services to be ready...${NC}"
sleep 30

# Health checks
echo -e "${BOLD}🏥 Running health checks...${NC}"

# Check if services are running
if curl -f http://localhost:8081/health > /dev/null 2>&1; then
    check_status 0 "RateWatch service healthy"
else
    check_status 1 "RateWatch service health check failed"
fi

# Test rate limiting functionality
if curl -f -X POST http://localhost:8081/v1/check \
    -H "Authorization: Bearer $(cat production_api_key.txt)" \
    -H "Content-Type: application/json" \
    -d '{"key":"deployment-test","limit":10,"window":60,"cost":1}' > /dev/null 2>&1; then
    check_status 0 "Rate limiting functionality working"
else
    check_status 1 "Rate limiting functionality test failed"
fi

# Create monitoring dashboards
echo -e "${BOLD}📊 Setting up monitoring...${NC}"

# Wait for Grafana to be ready
sleep 10

# Create Grafana dashboard
curl -X POST http://admin:admin123@localhost:3000/api/dashboards/db \
  -H "Content-Type: application/json" \
  -d '{
    "dashboard": {
      "title": "RateWatch Production Metrics",
      "panels": [
        {
          "title": "Request Rate",
          "type": "graph",
          "targets": [
            {
              "expr": "rate(ratewatch_requests_total[5m])",
              "legendFormat": "Requests/sec"
            }
          ]
        }
      ]
    }
  }' > /dev/null 2>&1

check_status $? "Monitoring dashboards configured"

# Create backup script
cat > scripts/backup.sh << 'EOF'
#!/bin/bash
# Backup script for RateWatch production data

BACKUP_DIR="/backup/$(date +%Y%m%d)"
mkdir -p $BACKUP_DIR

# Backup Redis data
docker exec redis-cluster redis-cli BGSAVE
docker cp redis-cluster:/data/dump.rdb $BACKUP_DIR/

# Backup configuration
cp .env.production $BACKUP_DIR/
cp docker-compose.production.yml $BACKUP_DIR/

echo "Backup completed: $BACKUP_DIR"
EOF

chmod +x scripts/backup.sh

# Create monitoring script
cat > scripts/monitor.sh << 'EOF'
#!/bin/bash
# Production monitoring script

echo "🔍 RateWatch Production Status"
echo "=============================="

# Check service health
echo "Service Health:"
curl -s http://localhost:8081/health | jq .

# Check metrics
echo -e "\nKey Metrics:"
curl -s http://localhost:8081/metrics | grep -E "(ratewatch_requests_total|ratewatch_request_duration)"

# Check container status
echo -e "\nContainer Status:"
docker-compose -f docker-compose.production.yml ps

# Check resource usage
echo -e "\nResource Usage:"
docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}"
EOF

chmod +x scripts/monitor.sh

# Final deployment summary
echo
echo -e "${BOLD}🎉 RateWatch SaaS Deployment Complete!${NC}"
echo "=============================================="
echo
echo -e "${GREEN}✅ All services deployed and healthy${NC}"
echo
echo -e "${BLUE}📊 Service URLs:${NC}"
echo "  • Application: https://$DOMAIN"
echo "  • Dashboard: https://$DOMAIN/dashboard"
echo "  • Metrics: https://$DOMAIN/metrics"
echo "  • Grafana: http://localhost:3000 (admin/admin123)"
echo "  • Prometheus: http://localhost:9090"
echo
echo -e "${BLUE}🔑 API Key:${NC}"
echo "  • Production API Key: $(cat production_api_key.txt)"
echo "  • Keep this secure and share only with authorized users"
echo
echo -e "${BLUE}🛠️  Management Commands:${NC}"
echo "  • Monitor: ./scripts/monitor.sh"
echo "  • Backup: ./scripts/backup.sh"
echo "  • Logs: docker-compose -f docker-compose.production.yml logs -f"
echo "  • Scale: docker-compose -f docker-compose.production.yml up -d --scale ratewatch=5"
echo
echo -e "${BLUE}📋 Next Steps:${NC}"
echo "  1. Configure DNS to point $DOMAIN to this server"
echo "  2. Set up automated backups (cron job for ./scripts/backup.sh)"
echo "  3. Configure monitoring alerts in Grafana"
echo "  4. Test customer onboarding flow"
echo "  5. Set up billing integration (Stripe)"
echo
echo -e "${YELLOW}⚠️  Important:${NC}"
echo "  • Keep production_api_key.txt secure"
echo "  • Monitor resource usage and scale as needed"
echo "  • Set up automated SSL certificate renewal"
echo "  • Configure firewall rules for production"
echo
echo -e "${GREEN}🚀 RateWatch SaaS is now live and ready for customers!${NC}"