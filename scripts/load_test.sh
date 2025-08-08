#!/bin/bash

# RateWatch Load Testing Script
# Tests performance under load

set -e

BASE_URL="${1:-http://localhost:8081}"
API_KEY="${2:-$(cat api_key.txt 2>/dev/null || echo 'rw_1754649262_8d587d1e227c50f4cca1a79934f51385')}"
CONCURRENT_USERS="${3:-50}"
DURATION="${4:-60s}"

echo "🔥 RateWatch Load Testing"
echo "========================"
echo "Base URL: $BASE_URL"
echo "Concurrent Users: $CONCURRENT_USERS"
echo "Duration: $DURATION"
echo ""

# Check if wrk is installed
if ! command -v wrk &> /dev/null; then
    echo "Installing wrk load testing tool..."
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        sudo apt-get update && sudo apt-get install -y wrk
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        brew install wrk
    else
        echo "❌ Please install wrk manually: https://github.com/wg/wrk"
        exit 1
    fi
fi

# Create test script for wrk
cat > /tmp/rate_limit_test.lua << 'EOF'
wrk.method = "POST"
wrk.body   = '{"key": "load-test-user", "limit": 1000, "window": 60, "cost": 1}'
wrk.headers["Content-Type"] = "application/json"
wrk.headers["Authorization"] = "Bearer API_KEY_PLACEHOLDER"

local counter = 1
local threads = {}

function setup(thread)
   thread:set("id", counter)
   table.insert(threads, thread)
   counter = counter + 1
end

function init(args)
   requests  = 0
   responses = 0
   
   local msg = "thread %d created"
   print(msg:format(id))
end

function request()
   requests = requests + 1
   return wrk.request()
end

function response(status, headers, body)
   responses = responses + 1
end
EOF

# Replace API key in test script
sed -i "s/API_KEY_PLACEHOLDER/$API_KEY/g" /tmp/rate_limit_test.lua

echo "1. Starting load test..."
echo "Running $CONCURRENT_USERS concurrent users for $DURATION"
echo ""

# Run load test
wrk -t12 -c$CONCURRENT_USERS -d$DURATION -s /tmp/rate_limit_test.lua $BASE_URL/v1/check

echo ""
echo "2. Testing dashboard under load..."
wrk -t4 -c10 -d30s $BASE_URL/dashboard

echo ""
echo "3. Testing health endpoint..."
wrk -t2 -c5 -d10s $BASE_URL/health

echo ""
echo "4. Getting final metrics..."
curl -s $BASE_URL/metrics | grep -E "(ratewatch_|http_)" | head -20

echo ""
echo "🎉 Load testing completed!"
echo ""
echo "📊 Check the dashboard for real-time metrics: $BASE_URL/dashboard"
echo "📈 Prometheus metrics available at: $BASE_URL/metrics"

# Cleanup
rm -f /tmp/rate_limit_test.lua
