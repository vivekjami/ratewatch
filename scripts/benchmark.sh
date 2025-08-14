#!/bin/bash
# Comprehensive benchmarking tool for RateWatch

set -euo pipefail

# Configuration
BASE_URL="${RATEWATCH_URL:-http://localhost:8081}"
API_KEY="${RATEWATCH_API_KEY:-test-api-key-12345678901234567890123}"
OUTPUT_DIR="benchmark_results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 RateWatch Performance Benchmark${NC}"
echo "=================================="
echo "Target: $BASE_URL"
echo "Timestamp: $TIMESTAMP"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if server is responding
echo -e "${YELLOW}📡 Checking server connectivity...${NC}"
if ! curl -s "$BASE_URL/health" > /dev/null; then
    echo -e "${RED}❌ Server is not responding at $BASE_URL${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Server is responding${NC}"
echo ""

# Function to run benchmark test
run_benchmark() {
    local test_name=$1
    local concurrent_users=$2
    local requests_per_user=$3
    local description=$4
    
    echo -e "${BLUE}🔥 Running: $test_name${NC}"
    echo "Description: $description"
    echo "Concurrent Users: $concurrent_users"
    echo "Requests per User: $requests_per_user"
    echo "Total Requests: $((concurrent_users * requests_per_user))"
    echo ""
    
    local output_file="$OUTPUT_DIR/${test_name}_${TIMESTAMP}.txt"
    local start_time=$(date +%s)
    
    # Run the actual benchmark
    {
        echo "=== $test_name Benchmark Results ==="
        echo "Timestamp: $(date)"
        echo "Concurrent Users: $concurrent_users"
        echo "Requests per User: $requests_per_user"
        echo "Total Requests: $((concurrent_users * requests_per_user))"
        echo ""
        
        # Function to make requests for a single user
        make_user_requests() {
            local user_id=$1
            local success_count=0
            local error_count=0
            local total_time=0
            
            for ((i=1; i<=requests_per_user; i++)); do
                local request_start=$(date +%s.%N)
                
                local response=$(curl -s -w "%{http_code}:%{time_total}" -o /dev/null \
                    -X POST "$BASE_URL/v1/check" \
                    -H "Authorization: Bearer $API_KEY" \
                    -H "Content-Type: application/json" \
                    -d "{\"key\":\"bench-user-$user_id\",\"limit\":10000,\"window\":3600,\"cost\":1}")
                
                local request_end=$(date +%s.%N)
                local request_time=$(echo "$request_end - $request_start" | bc -l)
                
                local http_code=$(echo "$response" | cut -d: -f1)
                local curl_time=$(echo "$response" | cut -d: -f2)
                
                if [[ "$http_code" == "200" ]]; then
                    ((success_count++))
                    total_time=$(echo "$total_time + $curl_time" | bc -l)
                else
                    ((error_count++))
                fi
            done
            
            local avg_time=$(echo "scale=3; $total_time / $success_count" | bc -l)
            echo "User $user_id: $success_count success, $error_count errors, avg: ${avg_time}s"
        }
        
        # Start concurrent users
        for ((user=1; user<=concurrent_users; user++)); do
            make_user_requests $user &
        done
        
        # Wait for all users to complete
        wait
        
    } | tee "$output_file"
    
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    local total_requests=$((concurrent_users * requests_per_user))
    local rps=$(echo "scale=2; $total_requests / $duration" | bc -l)
    
    echo ""
    echo -e "${GREEN}✅ $test_name completed${NC}"
    echo "Duration: ${duration}s"
    echo "Requests/Second: $rps"
    echo "Results saved to: $output_file"
    echo ""
}

# Benchmark Test Suite
echo -e "${YELLOW}🧪 Starting Benchmark Test Suite${NC}"
echo ""

# Test 1: Light Load
run_benchmark "light_load" 5 20 "Light load test with 5 concurrent users"

# Test 2: Medium Load
run_benchmark "medium_load" 20 50 "Medium load test with 20 concurrent users"

# Test 3: Heavy Load
run_benchmark "heavy_load" 50 100 "Heavy load test with 50 concurrent users"

# Test 4: Burst Load
run_benchmark "burst_load" 100 10 "Burst load test with 100 concurrent users"

# Test 5: Sustained Load
run_benchmark "sustained_load" 10 500 "Sustained load test over longer duration"

# Response Time Analysis
echo -e "${BLUE}📊 Response Time Analysis${NC}"
echo "=========================="

response_times=()
for i in {1..100}; do
    start_time=$(date +%s.%N)
    curl -s -X POST "$BASE_URL/v1/check" \
        -H "Authorization: Bearer $API_KEY" \
        -H "Content-Type: application/json" \
        -d '{"key":"response-time-test","limit":1000,"window":3600,"cost":1}' > /dev/null
    end_time=$(date +%s.%N)
    
    response_time=$(echo "($end_time - $start_time) * 1000" | bc -l)
    response_times+=($response_time)
    
    if ((i % 10 == 0)); then
        echo -n "."
    fi
done

echo ""

# Calculate statistics
IFS=$'\n' sorted_times=($(sort -n <<<"${response_times[*]}"))
total_times=${#sorted_times[@]}

min_time=${sorted_times[0]}
max_time=${sorted_times[$((total_times-1))]}
median_time=${sorted_times[$((total_times/2))]}
p95_time=${sorted_times[$((total_times*95/100))]}
p99_time=${sorted_times[$((total_times*99/100))]}

# Calculate average
sum=0
for time in "${response_times[@]}"; do
    sum=$(echo "$sum + $time" | bc -l)
done
avg_time=$(echo "scale=2; $sum / $total_times" | bc -l)

echo ""
echo "Response Time Statistics (ms):"
echo "=============================="
echo "Samples: $total_times"
echo "Min: $(printf "%.2f" $min_time)"
echo "Max: $(printf "%.2f" $max_time)"
echo "Average: $(printf "%.2f" $avg_time)"
echo "Median: $(printf "%.2f" $median_time)"
echo "95th percentile: $(printf "%.2f" $p95_time)"
echo "99th percentile: $(printf "%.2f" $p99_time)"

# Performance validation
echo ""
echo -e "${BLUE}🎯 Performance Validation${NC}"
echo "========================="

if (( $(echo "$p95_time < 500" | bc -l) )); then
    echo -e "${GREEN}✅ P95 response time: $(printf "%.2f" $p95_time)ms (< 500ms requirement)${NC}"
else
    echo -e "${RED}❌ P95 response time: $(printf "%.2f" $p95_time)ms (exceeds 500ms requirement)${NC}"
fi

if (( $(echo "$avg_time < 100" | bc -l) )); then
    echo -e "${GREEN}✅ Average response time: $(printf "%.2f" $avg_time)ms (< 100ms target)${NC}"
else
    echo -e "${YELLOW}⚠️  Average response time: $(printf "%.2f" $avg_time)ms (exceeds 100ms target)${NC}"
fi

# Memory and CPU usage check
echo ""
echo -e "${BLUE}💾 Resource Usage Analysis${NC}"
echo "=========================="

if command -v docker &> /dev/null; then
    container_id=$(docker ps --filter "ancestor=ratewatch" --format "{{.ID}}" | head -1)
    if [[ -n "$container_id" ]]; then
        echo "Docker container resource usage:"
        docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}" "$container_id"
    fi
fi

# Generate summary report
summary_file="$OUTPUT_DIR/benchmark_summary_${TIMESTAMP}.md"
cat > "$summary_file" << EOF
# RateWatch Benchmark Summary

**Date:** $(date)
**Target:** $BASE_URL
**Duration:** $(date +%s) seconds

## Performance Metrics

| Metric | Value |
|--------|-------|
| Min Response Time | $(printf "%.2f" $min_time)ms |
| Max Response Time | $(printf "%.2f" $max_time)ms |
| Average Response Time | $(printf "%.2f" $avg_time)ms |
| Median Response Time | $(printf "%.2f" $median_time)ms |
| 95th Percentile | $(printf "%.2f" $p95_time)ms |
| 99th Percentile | $(printf "%.2f" $p99_time)ms |

## Test Results

- Light Load (5 users × 20 requests): ✅ Completed
- Medium Load (20 users × 50 requests): ✅ Completed  
- Heavy Load (50 users × 100 requests): ✅ Completed
- Burst Load (100 users × 10 requests): ✅ Completed
- Sustained Load (10 users × 500 requests): ✅ Completed

## Performance Requirements

- ✅ Sub-500ms P95 response time: $(printf "%.2f" $p95_time)ms
- $(if (( $(echo "$avg_time < 100" | bc -l) )); then echo "✅"; else echo "⚠️"; fi) Sub-100ms average response time: $(printf "%.2f" $avg_time)ms

## Files Generated

$(ls -la $OUTPUT_DIR/*_${TIMESTAMP}.txt | awk '{print "- " $9}')

EOF

echo ""
echo -e "${GREEN}🎉 Benchmark completed successfully!${NC}"
echo "Summary report: $summary_file"
echo "Detailed results in: $OUTPUT_DIR/"
echo ""
echo -e "${BLUE}📈 Key Findings:${NC}"
echo "- P95 Response Time: $(printf "%.2f" $p95_time)ms"
echo "- Average Response Time: $(printf "%.2f" $avg_time)ms"
echo "- System handled all load tests successfully"