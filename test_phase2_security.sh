#!/bin/bash

echo "=== RateWatch Phase 2 Security Testing ==="
echo

# Test unauthorized access
echo "1. Testing unauthorized access (should fail):"
curl -s -o /dev/null -w "Status: %{http_code}\n" -X POST http://localhost:8081/v1/check \
    -H "Content-Type: application/json" \
    -d '{"key": "user:test", "limit": 10, "window": 60, "cost": 1}'
echo

# Test invalid API key
echo "2. Testing invalid API key (too short):"
curl -s -o /dev/null -w "Status: %{http_code}\n" -X POST http://localhost:8081/v1/check \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer short" \
    -d '{"key": "user:test", "limit": 10, "window": 60, "cost": 1}'
echo

# Test valid API key
echo "3. Testing valid API key (should work):"
curl -s -X POST http://localhost:8081/v1/check \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer test-api-key-32-characters-long-test" \
    -d '{"key": "user:security_test", "limit": 5, "window": 60, "cost": 1}' | jq '.'
echo

# Test security headers
echo "4. Testing security headers:"
echo "Security headers present:"
curl -s -I http://localhost:8081/health | grep -E "(x-frame-options|x-content-type-options|strict-transport-security|x-xss-protection|referrer-policy)"
echo

# Test GDPR data summary (before creating data)
echo "5. Testing GDPR data summary (no data):"
curl -s -X POST http://localhost:8081/v1/privacy/summary \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer test-api-key-32-characters-long-test" \
    -d '{"user_id": "user:gdpr_test"}' | jq '.'
echo

# Create some data for GDPR testing
echo "6. Creating data for GDPR testing:"
for i in {1..3}; do
    echo "Request $i:"
    curl -s -X POST http://localhost:8081/v1/check \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer test-api-key-32-characters-long-test" \
        -d '{"key": "user:gdpr_test", "limit": 10, "window": 60, "cost": 1}' | jq '.remaining'
done
echo

# Test GDPR data summary (after creating data)
echo "7. Testing GDPR data summary (with data):"
curl -s -X POST http://localhost:8081/v1/privacy/summary \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer test-api-key-32-characters-long-test" \
    -d '{"user_id": "user:gdpr_test"}' | jq '.'
echo

# Test GDPR data deletion
echo "8. Testing GDPR data deletion:"
curl -s -X POST http://localhost:8081/v1/privacy/delete \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer test-api-key-32-characters-long-test" \
    -d '{"user_id": "user:gdpr_test", "reason": "user_request"}' | jq '.'
echo

# Verify data deletion
echo "9. Verifying data deletion:"
curl -s -X POST http://localhost:8081/v1/privacy/summary \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer test-api-key-32-characters-long-test" \
    -d '{"user_id": "user:gdpr_test"}' | jq '.'
echo

# Test detailed health check
echo "10. Testing detailed health check:"
curl -s http://localhost:8081/health/detailed | jq '.'
echo

echo "=== Phase 2 Security Testing Complete ==="
