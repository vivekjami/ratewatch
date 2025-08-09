# RateWatch - Production Release Guide

This guide provides a step-by-step approach to release RateWatch for production use **without any costs** for licenses, API keys, or commercial services.

## 1. Free Production Deployment Options

### Option A: Free Oracle Cloud Deployment (Recommended - Always Free)

Oracle Cloud offers a completely free VM that never expires:

```bash
# 1. Sign up for Oracle Cloud Free Tier (no credit card required)
#    Visit: https://www.oracle.com/cloud/free/

# 2. Create an "Always Free" VM with Ubuntu
#    - Select "Always Free Eligible" resources
#    - Choose "Ubuntu 20.04" or newer
#    - Use the smallest VM shape (it's free forever)

# 3. SSH into your VM

# 4. Install dependencies
sudo apt update
sudo apt install -y docker.io docker-compose git

# 5. Clone your repository
git clone https://github.com/yourusername/ratewatch.git
cd ratewatch

# 6. Start Redis
docker-compose up -d redis

# 7. Generate a secure API key (FREE - no external service needed)
API_KEY=$(openssl rand -hex 16)
echo "rw_$(date +%s)_$API_KEY" > api_key.txt
echo "Your API key: $(cat api_key.txt)"

# 8. Build and run RateWatch
cargo build --release
nohup ./target/release/ratewatch &
```

### Option B: Free Render.com Deployment

Render offers a free tier with automatic deployments from GitHub:

1. Sign up at [render.com](https://render.com) (no credit card needed)
2. Connect your GitHub repo
3. Create a "Web Service" pointing to your repo
4. Set build command: `cargo build --release`
5. Set start command: `./target/release/ratewatch`
6. Add environment variables:
   - `REDIS_URL=redis://internal-redis:6379`
7. Create a free Redis instance in your Render dashboard

## 2. Making RateWatch Publicly Accessible (FREE)

### Using Cloudflare Tunnel (100% Free)

Cloudflare offers free tunnels to make your service public without paying for static IPs:

```bash
# 1. Sign up for free Cloudflare account
#    Visit: https://dash.cloudflare.com/sign-up

# 2. Install cloudflared
curl -L --output cloudflared.deb https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb
sudo dpkg -i cloudflared.deb

# 3. Authenticate and create tunnel
cloudflared tunnel login
cloudflared tunnel create ratewatch

# 4. Configure tunnel to point to your RateWatch service
# Create config file: ~/.cloudflared/config.yml
cat > ~/.cloudflared/config.yml << EOF
tunnel: YOUR_TUNNEL_ID
credentials-file: /root/.cloudflared/YOUR_TUNNEL_ID.json
ingress:
  - hostname: ratewatch.yourdomain.com
    service: http://localhost:8080
  - service: http_status:404
EOF

# 5. Add DNS record through Cloudflare dashboard
# - Type: CNAME
# - Name: ratewatch
# - Target: YOUR_TUNNEL_ID.cfargotunnel.com

# 6. Run the tunnel
cloudflared tunnel run ratewatch
```

## 3. Setting Up Free API Keys (No Paid Services)

RateWatch generates its own API keys - no paid service required:

```bash
# Generate a secure API key
API_KEY="rw_$(date +%s)_$(openssl rand -hex 16)"
echo $API_KEY > api_key.txt
echo "Your API key: $API_KEY"

# For multiple users, generate different keys
USER1_KEY="rw_$(date +%s)_$(openssl rand -hex 16)"
USER2_KEY="rw_$(date +%s)_$(openssl rand -hex 16)"
echo "User 1 API key: $USER1_KEY"
echo "User 2 API key: $USER2_KEY"
```

## 4. Free Security & Compliance Setup

You can achieve excellent security without paying for any tools:

### Free SSL/TLS Certificates

```bash
# Install certbot
sudo apt install -y certbot

# Get free certificate
sudo certbot certonly --standalone -d ratewatch.yourdomain.com
```

### Free Security Headers (Already Implemented)

RateWatch already implements all OWASP recommended security headers:

- X-Frame-Options
- X-Content-Type-Options
- Content-Security-Policy
- Strict-Transport-Security

### GDPR Compliance (Free)

RateWatch has built-in GDPR endpoints for free:
- Data deletion: `/v1/privacy/delete`
- Data summary: `/v1/privacy/summary` 

## 5. Free Monitoring & Analytics

### Prometheus + Grafana (Free & Open Source)

```bash
# Start the monitoring stack (included in project)
cd monitoring
docker-compose up -d

# Access Grafana at http://your-server-ip:3000
# Default login: admin/admin
```

## 6. Release Preparation Checklist

Before your final release:

1. Clean up development files:
```bash
# Remove development-only files
rm -f test_*.sh
```

2. Secure your API keys:
```bash
# Make API keys readable only by owner
chmod 600 api_key.txt
```

3. Update documentation:
```bash
# Edit README.md to update any contact info
```

4. Final validation:
```bash
# Run validation script
./validate.sh

# If you see "31/31 checks passed (100%)" - you're ready!
```

## 7. Sharing Your API with Users (Free Distribution)

### Client Libraries (Free)

The client libraries are ready to use and distribute:

```bash
# Python client
cd clients/python
pip install -e .
# OR publish to PyPI (free)

# Node.js client 
cd clients/nodejs
npm link
# OR publish to npm (free)
```

### Free API Documentation

Host your API docs for free using GitHub Pages:

1. Create a `docs` folder in your repository
2. Move your API documentation there
3. Enable GitHub Pages in repository settings

## 8. Overdelivering on Value (Free Enhancements)

To make your product truly exceptional:

1. **Add a status page**:
```bash
# Create a simple status page
mkdir -p static/status
# Create status.html with uptime info
```

2. **Add load testing script**:
```bash
# Use the included load test script
./scripts/load_test.sh
```

3. **Create email alerts** (free):
```bash
# Set up free email alerts with Prometheus
# Edit monitoring/alertmanager.yml
```

## 9. Ongoing Maintenance (Free)

Maintain your service for free:

1. **Auto-updates**:
```bash
# Create a simple update script
cat > update.sh << EOF
#!/bin/bash
git pull
cargo build --release
systemctl restart ratewatch
EOF
chmod +x update.sh
```

2. **Backup automation**:
```bash
# Add to crontab
echo "0 2 * * * tar -czf ~/backups/ratewatch-\$(date +%Y%m%d).tar.gz ~/.ratewatch/data" | crontab -
```

3. **Health check automation**:
```bash
# Create a health check script
cat > health_check.sh << EOF
#!/bin/bash
if ! curl -s http://localhost:8080/health | grep -q "ok"; then
  systemctl restart ratewatch
fi
EOF
chmod +x health_check.sh
```

## 10. Free Alternative to Commercial Rate Limiting

With RateWatch, you now have a 100% free alternative to:

- Kong Enterprise ($450+/month)
- Tyk Cloud ($500+/month)
- AWS API Gateway ($1+/million requests)
- Cloudflare Rate Limiting ($5+/month)

🎉 **Congratulations!** You've successfully released a professional-grade rate limiting service with zero ongoing costs.
