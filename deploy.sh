#!/bin/bash

# RateWatch Production Deployment Script
# Usage: ./deploy.sh [domain] [email]
# Example: ./deploy.sh api.example.com admin@example.com

set -e

# Text formatting
BOLD="\e[1m"
GREEN="\e[32m"
YELLOW="\e[33m"
RED="\e[31m"
RESET="\e[0m"

# Display header
echo -e "${BOLD}📦 RateWatch Production Deployment${RESET}"
echo "========================================"
echo

# Check if running as root
if [ "$EUID" -ne 0 ] && [ "$1" != "--local" ]; then
  echo -e "${RED}Error: This script must be run as root unless using --local flag.${RESET}"
  echo "Try: sudo $0 $*"
  exit 1
fi

# Parse arguments
DOMAIN=$1
EMAIL=$2
LOCAL_ONLY=false

if [ "$1" == "--local" ]; then
  LOCAL_ONLY=true
  DOMAIN="localhost"
  shift
fi

if [ -z "$DOMAIN" ] && [ "$LOCAL_ONLY" == "false" ]; then
  echo -e "${YELLOW}No domain specified. Defaulting to local deployment.${RESET}"
  LOCAL_ONLY=true
  DOMAIN="localhost"
fi

# Function to check if a command exists
command_exists() {
  command -v "$1" >/dev/null 2>&1
}

# Setup phase
echo -e "${BOLD}🔧 Setting up environment...${RESET}"

# Check for Docker
if ! command_exists docker; then
  echo -e "${YELLOW}Docker not found. Installing Docker...${RESET}"
  curl -fsSL https://get.docker.com | sh
fi

# Check for Docker Compose
if ! command_exists docker-compose; then
  echo -e "${YELLOW}Docker Compose not found. Installing Docker Compose...${RESET}"
  curl -L "https://github.com/docker/compose/releases/download/v2.10.0/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose
  chmod +x /usr/local/bin/docker-compose
fi

# Generate secure secrets
echo -e "${BOLD}🔒 Generating secure credentials...${RESET}"
API_KEY_SECRET=$(openssl rand -hex 32)
REDIS_PASSWORD=$(openssl rand -hex 16)

# Create .env.production file
echo -e "${BOLD}📝 Creating environment configuration...${RESET}"
cat > .env.production << EOF
# RateWatch Production Configuration
PORT=8080
RUST_LOG=info
REDIS_URL=redis://:${REDIS_PASSWORD}@redis:6379
API_KEY_SECRET=${API_KEY_SECRET}
EOF

# Generate API key
echo -e "${BOLD}🔑 Generating API key...${RESET}"
API_KEY="rw_$(date +%s)_$(openssl rand -hex 16)"
echo -n "$API_KEY" > api_key.txt
chmod 600 api_key.txt

echo -e "${GREEN}API key generated and saved to api_key.txt${RESET}"
echo -e "${YELLOW}IMPORTANT: Keep this key secure!${RESET}"

# Deployment type
if [ "$LOCAL_ONLY" == "true" ]; then
  echo -e "${BOLD}🚀 Deploying locally...${RESET}"
  
  # Start Redis and application
  echo -e "${BOLD}📦 Starting services...${RESET}"
  docker-compose -f docker-compose.prod.yml up -d
  
  echo -e "${GREEN}✅ Local deployment complete!${RESET}"
  echo
  echo -e "📊 RateWatch is running at: ${BOLD}http://localhost:8080${RESET}"
  echo -e "📈 Dashboard available at: ${BOLD}http://localhost:8080/dashboard${RESET}"
  echo -e "❤️  Health check: ${BOLD}http://localhost:8080/health${RESET}"
  echo
  echo -e "${YELLOW}To use your API key:${RESET}"
  echo "curl -X POST http://localhost:8080/v1/check \\"
  echo "  -H \"Authorization: Bearer $(cat api_key.txt)\" \\"
  echo "  -H \"Content-Type: application/json\" \\"
  echo "  -d '{\"key\": \"user:123\", \"limit\": 100, \"window\": 3600, \"cost\": 1}'"
  
else
  echo -e "${BOLD}🌎 Deploying to production for domain: ${DOMAIN}...${RESET}"
  
  # Check for certbot
  if ! command_exists certbot; then
    echo -e "${YELLOW}Certbot not found. Installing Certbot...${RESET}"
    apt-get update
    apt-get install -y certbot
  fi
  
  # Create nginx config
  echo -e "${BOLD}🔧 Creating Nginx configuration...${RESET}"
  mkdir -p nginx
  cat > nginx/ratewatch.conf << EOF
server {
    listen 80;
    server_name ${DOMAIN};
    
    location /.well-known/acme-challenge/ {
        root /var/www/certbot;
    }
    
    location / {
        return 301 https://\$host\$request_uri;
    }
}

server {
    listen 443 ssl;
    server_name ${DOMAIN};
    
    ssl_certificate /etc/letsencrypt/live/${DOMAIN}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/${DOMAIN}/privkey.pem;
    
    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Content-Type-Options nosniff always;
    add_header X-Frame-Options DENY always;
    add_header X-XSS-Protection "1; mode=block" always;
    
    location / {
        proxy_pass http://ratewatch:8080;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
EOF
  
  # Create directories for certbot
  mkdir -p data/certbot/conf
  mkdir -p data/certbot/www
  
  # Get SSL certificate
  echo -e "${BOLD}🔒 Obtaining SSL certificate...${RESET}"
  certbot certonly --webroot -w data/certbot/www -d ${DOMAIN} --email ${EMAIL} --agree-tos --no-eff-email
  
  # Start services
  echo -e "${BOLD}📦 Starting services...${RESET}"
  docker-compose -f docker-compose.prod.yml up -d
  
  echo -e "${GREEN}✅ Production deployment complete!${RESET}"
  echo
  echo -e "📊 RateWatch is running at: ${BOLD}https://${DOMAIN}${RESET}"
  echo -e "📈 Dashboard available at: ${BOLD}https://${DOMAIN}/dashboard${RESET}"
  echo -e "❤️  Health check: ${BOLD}https://${DOMAIN}/health${RESET}"
  echo
  echo -e "${YELLOW}To use your API key:${RESET}"
  echo "curl -X POST https://${DOMAIN}/v1/check \\"
  echo "  -H \"Authorization: Bearer $(cat api_key.txt)\" \\"
  echo "  -H \"Content-Type: application/json\" \\"
  echo "  -d '{\"key\": \"user:123\", \"limit\": 100, \"window\": 3600, \"cost\": 1}'"
fi

echo
echo -e "${BOLD}📝 Post-Deployment Steps${RESET}"
echo "1. Keep your API key (api_key.txt) secure"
echo "2. Set up monitoring with: docker-compose -f monitoring/docker-compose.yml up -d"
echo "3. Run the validation script: ./validate.sh"
echo
echo -e "${GREEN}🎉 Deployment Successful!${RESET}"
