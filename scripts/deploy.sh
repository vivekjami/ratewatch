#!/bin/bash

# RateWatch Production Deployment Script
# Deploys RateWatch to production with all necessary components

set -e

echo "🚀 RateWatch Production Deployment"
echo "=================================="

# Configuration
DOMAIN="${1:-your-domain.com}"
EMAIL="${2:-admin@your-domain.com}"
ENVIRONMENT="${3:-production}"

echo "Domain: $DOMAIN"
echo "Email: $EMAIL"
echo "Environment: $ENVIRONMENT"
echo ""

# Check prerequisites
echo "1. Checking prerequisites..."
command -v docker >/dev/null 2>&1 || { echo "❌ Docker is required but not installed."; exit 1; }
command -v docker-compose >/dev/null 2>&1 || { echo "❌ Docker Compose is required but not installed."; exit 1; }
echo "✅ Prerequisites satisfied"

# Generate secure secrets
echo ""
echo "2. Generating secure secrets..."
API_KEY_SECRET=$(openssl rand -base64 32)
REDIS_PASSWORD=$(openssl rand -base64 16)

# Create environment file
cat > .env.prod << EOF
# Generated on $(date)
API_KEY_SECRET=$API_KEY_SECRET
REDIS_PASSWORD=$REDIS_PASSWORD
RUST_LOG=info
DOMAIN=$DOMAIN
EMAIL=$EMAIL
EOF

echo "✅ Secrets generated and saved to .env.prod"

# Update nginx configuration with domain
echo ""
echo "3. Configuring nginx..."
sed "s/your-domain.com/$DOMAIN/g" deploy/nginx.prod.conf > deploy/nginx.conf
echo "✅ Nginx configured for domain: $DOMAIN"

# Build and start services
echo ""
echo "4. Building and starting services..."
docker-compose -f docker-compose.prod.yml --env-file .env.prod up -d --build

# Wait for services to start
echo ""
echo "5. Waiting for services to start..."
sleep 30

# Health check
echo ""
echo "6. Running health checks..."
if curl -f http://localhost:8081/health >/dev/null 2>&1; then
    echo "✅ RateWatch service is healthy"
else
    echo "❌ RateWatch service health check failed"
    exit 1
fi

# Setup SSL with Let's Encrypt (if domain is not localhost)
if [ "$DOMAIN" != "localhost" ] && [ "$DOMAIN" != "127.0.0.1" ]; then
    echo ""
    echo "7. Setting up SSL with Let's Encrypt..."
    docker run --rm \
        -v /etc/letsencrypt:/etc/letsencrypt \
        -v /var/lib/letsencrypt:/var/lib/letsencrypt \
        -p 80:80 \
        certbot/certbot certonly \
        --standalone \
        --email $EMAIL \
        --agree-tos \
        --no-eff-email \
        -d $DOMAIN
    
    echo "✅ SSL certificate obtained"
    
    # Restart nginx with SSL
    docker-compose -f docker-compose.prod.yml restart nginx
    echo "✅ Nginx restarted with SSL"
fi

# Generate first API key
echo ""
echo "8. Generating first API key..."
FIRST_API_KEY="rw_$(date +%s)_$(openssl rand -hex 16)"
echo "$FIRST_API_KEY" > api_key.txt
echo "✅ First API key generated: $FIRST_API_KEY"

# Final test
echo ""
echo "9. Running final integration test..."
API_KEY=$FIRST_API_KEY ./test.sh

echo ""
echo "🎉 Deployment completed successfully!"
echo ""
echo "📋 Deployment Summary:"
echo "======================"
echo "🌐 Dashboard: https://$DOMAIN/dashboard"
echo "🔗 API Endpoint: https://$DOMAIN/v1/check"
echo "📊 Metrics: https://$DOMAIN/metrics"
echo "❤️  Health: https://$DOMAIN/health"
echo ""
echo "🔑 Your API Key: $FIRST_API_KEY"
echo "📁 Secrets saved to: .env.prod"
echo ""
echo "📚 Next Steps:"
echo "1. Test your API with: curl -X POST https://$DOMAIN/v1/check -H 'Authorization: Bearer $FIRST_API_KEY' -H 'Content-Type: application/json' -d '{\"key\":\"test\",\"limit\":10,\"window\":60,\"cost\":1}'"
echo "2. Set up monitoring: docker-compose -f monitoring/docker-compose.yml up -d"
echo "3. Configure alerts in monitoring/alertmanager.yml"
echo "4. Set up backups for Redis data"
echo ""
echo "🛡️  Security Notes:"
echo "- Change default passwords in .env.prod"
echo "- Review nginx security headers"
echo "- Set up firewall rules"
echo "- Enable automatic security updates"
