#!/bin/bash
# Load test script for RateWatch

set -euo pipefail

# Configuration
BASE_URL="${RATEWATCH_URL:-http://localhost:8081}"
API_KEY="${RATEWATCH_API_KEY:-test-api-key-12345678901234567890123}"
CONCURRENT_USERS="${CONCURRENT_USERS:-10}"
REQUESTS_PER_USER="${REQUESTS_PER_USER:-100}"
DURATION="${DURATION:-60}"

echo "🚀 Starting RateWatch Load Test"
echo "================================"
echo "Base URL: $BASE_URL"
echo "Concurrent Users: $CONCURRENT_USERS"
echo "Requests per User: $REQUESTS_PER_USER"
echo "Duration: ${DURATION}s"
echo ""

# Check if server is running
if ! curl -s "$BASE_URL/health" > /dev/null; then
    echo "❌ RateWatch server is not responding at $BASE_URL"
    exit 1
fi

echo "✅ Server is responding"

# Function to make requests
make_requests() {
    local user_id=$1
    local success_count=0
    local error_count=0
    
    for ((i=1; i<=REQUESTS_PER_USER; i++)); do
        response=$(curl -s -w "%{http_code}" -o /dev/null \
            -X POST "$BASE_URL/v1/check" \
            -H "Authorization: Bearer $API_KEY" \
            -H "Content-Type: application/json" \
            -d "{\"key\":\"load-test-user-$user_id\",\"limit\":1000,\"window\":3600,\"cost\":1}")
        
        if [[ "$response" == "200" ]]; then
            ((success_count++))
        else
            ((error_count++))
        fi
        
        # Small delay to avoid overwhelming the server
        sleep 0.01
    done
    
    echo "User $user_id: $success_count success, $error_count errors"
}

# Start load test
echo "🔥 Starting load test..."
start_time=$(date +%s)

# Run concurrent users
for ((user=1; user<=CONCURRENT_USERS; user++)); do
    make_requests $user &
done

# Wait for all background jobs to complete
wait

end_time=$(date +%s)
duration=$((end_time - start_time))

echo ""
echo "📊 Load Test Results"
echo "===================="
echo "Total Duration: ${duration}s"
echo "Total Requests: $((CONCURRENT_USERS * REQUESTS_PER_USER))"
echo "Requests/Second: $(( (CONCURRENT_USERS * REQUESTS_PER_USER) / duration ))"

echo ""
echo "✅ Load test completed!"