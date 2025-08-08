#!/bin/bash

# RateWatch Integration Test Script
# Tests all core functionality after deployment

set -e

BASE_URL="${1:-http://localhost:8081}"
API_KEY="${2:-$(cat api_key.txt 2>/dev/null || echo 'rw_1754649262_8d587d1e227c50f4cca1a79934f51385')}"

echo "🧪 RateWatch Integration Tests"
echo "Base URL: $BASE_URL"
echo "================================"

# Test 1: Health Check
echo "1. Testing health endpoint..."
HEALTH_RESPONSE=$(curl -s "$BASE_URL/health")
if echo "$HEALTH_RESPONSE" | grep -q "ok"; then
    echo "✅ Health check passed"
else
    echo "❌ Health check failed"
    exit 1
fi

# Test 2: Dashboard Access
echo "2. Testing dashboard access..."
DASHBOARD_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/dashboard")
if [ "$DASHBOARD_RESPONSE" = "200" ]; then
    echo "✅ Dashboard accessible"
else
    echo "❌ Dashboard failed (HTTP $DASHBOARD_RESPONSE)"
    exit 1
fi

# Test 3: Rate Limiting (Simple)
echo "3. Testing rate limiting..."
RATE_LIMIT_RESPONSE=$(curl -s -X POST "$BASE_URL/v1/check" \
    -H "Authorization: Bearer $API_KEY" \
    -H "Content-Type: application/json" \
    -d '{
        "key": "test-user",
        "limit": 10,
        "window": 60,
        "cost": 1
    }')

if echo "$RATE_LIMIT_RESPONSE" | grep -q "allowed"; then
    echo "✅ Rate limiting working"
else
    echo "❌ Rate limiting failed"
    echo "Response: $RATE_LIMIT_RESPONSE"
    exit 1
fi

# Test 4: Analytics Endpoints
echo "4. Testing analytics..."
ANALYTICS_RESPONSE=$(curl -s -H "Authorization: Bearer $API_KEY" "$BASE_URL/v1/analytics/stats")
if echo "$ANALYTICS_RESPONSE" | grep -q "requests"; then
    echo "✅ Analytics working"
else
    echo "❌ Analytics failed"
    exit 1
fi

# Test 5: Detailed Health Check
echo "5. Testing detailed health..."
DETAILED_HEALTH=$(curl -s "$BASE_URL/health/detailed")
if echo "$DETAILED_HEALTH" | grep -q "redis"; then
    echo "✅ Detailed health check passed"
else
    echo "❌ Detailed health check failed"
    exit 1
fi

# Test 6: Load Test (10 requests)
echo "6. Running mini load test..."
for i in {1..10}; do
    curl -s -X POST "$BASE_URL/v1/check" \
        -H "Authorization: Bearer $API_KEY" \
        -H "Content-Type: application/json" \
        -d "{\"key\": \"load-test-$i\", \"limit\": 100, \"window\": 60, \"cost\": 1}" > /dev/null
done
echo "✅ Load test completed"

echo ""
echo "🎉 All tests passed! RateWatch is working correctly."
echo ""
echo "📊 Dashboard: $BASE_URL/dashboard"
echo "🔗 API Docs: $BASE_URL/docs"
echo "❤️  Health: $BASE_URL/health"
