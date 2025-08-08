#!/bin/bash

echo "=== RateWatch Phase 1 Testing ==="
echo

# Test health endpoint
echo "1. Testing health endpoint:"
curl -s http://localhost:8081/health | jq '.'
echo
echo

# Test basic rate limiting
echo "2. Testing basic rate limiting (user:test1):"
for i in {1..3}; do
    echo "Request $i:"
    curl -s -X POST http://localhost:8081/v1/check \
        -H "Content-Type: application/json" \
        -d '{"key": "user:test1", "limit": 2, "window": 60, "cost": 1}' | jq '.'
    echo
done
echo

# Test different user isolation
echo "3. Testing user isolation (user:test2):"
curl -s -X POST http://localhost:8081/v1/check \
    -H "Content-Type: application/json" \
    -d '{"key": "user:test2", "limit": 5, "window": 60, "cost": 1}' | jq '.'
echo
echo

# Test higher cost
echo "4. Testing higher cost consumption:"
curl -s -X POST http://localhost:8081/v1/check \
    -H "Content-Type: application/json" \
    -d '{"key": "user:test3", "limit": 10, "window": 60, "cost": 5}' | jq '.'
echo
echo

# Test error handling
echo "5. Testing error handling:"
echo "Invalid JSON:"
curl -s -X POST http://localhost:8081/v1/check \
    -H "Content-Type: application/json" \
    -d '{"invalid": "json"}'
echo
echo

echo "=== Phase 1 Testing Complete ==="
