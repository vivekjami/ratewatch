#!/bin/bash

# RateWatch API Key Setup Script
# Creates API keys in Redis for testing

set -e

REDIS_URL="${1:-redis://127.0.0.1:6379}"
API_KEY="${2:-test-key-123}"

echo "🔑 Setting up API key for RateWatch"
echo "Redis URL: $REDIS_URL"
echo "API Key: $API_KEY"
echo "================================"

# Check if redis-cli is available
if ! command -v redis-cli &> /dev/null; then
    echo "❌ redis-cli not found. Please install redis-tools:"
    echo "   Ubuntu/Debian: sudo apt-get install redis-tools"
    echo "   MacOS: brew install redis"
    echo "   CentOS/RHEL: sudo yum install redis"
    exit 1
fi

# Test Redis connection
echo "Testing Redis connection..."
if redis-cli -u "$REDIS_URL" ping > /dev/null 2>&1; then
    echo "✅ Redis connection successful"
else
    echo "❌ Cannot connect to Redis at $REDIS_URL"
    echo "Please ensure Redis is running and accessible"
    exit 1
fi

# Generate API key hash using the same method as the Rust application
# We'll use Blake3 hash of the API key with the secret
API_KEY_SECRET="${API_KEY_SECRET:-change-this-in-production}"
echo "Using API_KEY_SECRET: $API_KEY_SECRET"

# For now, let's store the API key directly and let the application handle hashing
# Store the API key in Redis
redis-cli -u "$REDIS_URL" HSET "api_keys" "$API_KEY" "active"

# Verify the key was stored
if redis-cli -u "$REDIS_URL" HEXISTS "api_keys" "$API_KEY" | grep -q "1"; then
    echo "✅ API key stored successfully in Redis"
else
    echo "❌ Failed to store API key in Redis"
    exit 1
fi

echo ""
echo "🎉 API key setup complete!"
echo ""
echo "You can now use the API key: $API_KEY"
echo "Example usage:"
echo "curl -X POST http://localhost:8081/v1/check \\"
echo "  -H \"Authorization: Bearer $API_KEY\" \\"
echo "  -H \"Content-Type: application/json\" \\"
echo "  -d '{\"key\": \"test-user\", \"limit\": 10, \"window\": 60, \"cost\": 1}'"
