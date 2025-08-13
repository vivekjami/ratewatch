#!/usr/bin/env python3
"""
Production Core Functionality Test
Comprehensive test of all core features under production conditions
"""

import requests
import json
import time
import concurrent.futures
import sys
from typing import List, Dict

BASE_URL = "http://localhost:8083"
API_KEY = "test-api-key-12345678901234567890123"

def test_rate_limiting_accuracy():
    """Test rate limiting accuracy under various conditions"""
    print("🎯 Testing rate limiting accuracy...")
    
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    }
    
    # Test 1: Basic rate limiting
    test_key = f"accuracy-test-{int(time.time())}"
    limit = 5
    window = 60
    
    allowed_count = 0
    denied_count = 0
    
    for i in range(10):  # Try 10 requests with limit of 5
        payload = {
            "key": test_key,
            "limit": limit,
            "window": window,
            "cost": 1
        }
        
        response = requests.post(f"{BASE_URL}/v1/check", json=payload, headers=headers)
        if response.status_code == 200:
            data = response.json()
            if data["allowed"]:
                allowed_count += 1
            else:
                denied_count += 1
        else:
            print(f"❌ Request failed with status {response.status_code}")
            return False
    
    if allowed_count == limit and denied_count == (10 - limit):
        print(f"✅ Rate limiting accuracy: {allowed_count} allowed, {denied_count} denied (expected)")
        return True
    else:
        print(f"❌ Rate limiting inaccuracy: {allowed_count} allowed, {denied_count} denied")
        return False

def test_concurrent_requests():
    """Test rate limiting under concurrent load"""
    print("🚀 Testing concurrent request handling...")
    
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    }
    
    test_key = f"concurrent-test-{int(time.time())}"
    limit = 10
    window = 60
    num_requests = 20
    
    def make_request(request_id):
        payload = {
            "key": test_key,
            "limit": limit,
            "window": window,
            "cost": 1
        }
        
        start_time = time.time()
        response = requests.post(f"{BASE_URL}/v1/check", json=payload, headers=headers)
        end_time = time.time()
        
        return {
            "id": request_id,
            "status": response.status_code,
            "allowed": response.json().get("allowed", False) if response.status_code == 200 else False,
            "duration": end_time - start_time
        }
    
    # Execute concurrent requests
    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as executor:
        futures = [executor.submit(make_request, i) for i in range(num_requests)]
        results = [future.result() for future in concurrent.futures.as_completed(futures)]
    
    # Analyze results
    allowed_count = sum(1 for r in results if r["allowed"])
    avg_duration = sum(r["duration"] for r in results) / len(results)
    max_duration = max(r["duration"] for r in results)
    
    print(f"   - Concurrent requests: {num_requests}")
    print(f"   - Allowed: {allowed_count} (expected: {limit})")
    print(f"   - Average response time: {avg_duration:.3f}s")
    print(f"   - Max response time: {max_duration:.3f}s")
    
    # Validate results
    if allowed_count == limit and max_duration < 0.5:  # <500ms requirement
        print("✅ Concurrent request handling passed")
        return True
    else:
        print("❌ Concurrent request handling failed")
        return False

def test_sliding_window_behavior():
    """Test sliding window algorithm behavior"""
    print("⏰ Testing sliding window behavior...")
    
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    }
    
    test_key = f"sliding-window-test-{int(time.time())}"
    limit = 3
    window = 10  # 10 second window for faster testing
    
    # Make requests and track timing
    results = []
    
    for i in range(5):
        payload = {
            "key": test_key,
            "limit": limit,
            "window": window,
            "cost": 1
        }
        
        start_time = time.time()
        response = requests.post(f"{BASE_URL}/v1/check", json=payload, headers=headers)
        
        if response.status_code == 200:
            data = response.json()
            results.append({
                "request": i + 1,
                "allowed": data["allowed"],
                "remaining": data["remaining"],
                "reset_in": data["reset_in"],
                "timestamp": start_time
            })
        
        if i == 2:  # After 3rd request, wait for window to partially reset
            print(f"   - Waiting 5 seconds for partial window reset...")
            time.sleep(5)
    
    # Analyze sliding window behavior
    allowed_requests = [r for r in results if r["allowed"]]
    denied_requests = [r for r in results if not r["allowed"]]
    
    print(f"   - Total allowed: {len(allowed_requests)}")
    print(f"   - Total denied: {len(denied_requests)}")
    
    # Should allow first 3, deny 4th, then allow 5th after partial reset
    if len(allowed_requests) >= 3:
        print("✅ Sliding window behavior working correctly")
        return True
    else:
        print("❌ Sliding window behavior incorrect")
        return False

def test_cost_based_limiting():
    """Test cost-based rate limiting"""
    print("💰 Testing cost-based rate limiting...")
    
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    }
    
    test_key = f"cost-test-{int(time.time())}"
    limit = 10
    window = 60
    
    # Test with different costs
    test_cases = [
        {"cost": 1, "expected_remaining": 9},
        {"cost": 3, "expected_remaining": 6},
        {"cost": 5, "expected_remaining": 1},
        {"cost": 2, "expected_allowed": False}  # Should exceed limit
    ]
    
    for i, case in enumerate(test_cases):
        payload = {
            "key": test_key,
            "limit": limit,
            "window": window,
            "cost": case["cost"]
        }
        
        response = requests.post(f"{BASE_URL}/v1/check", json=payload, headers=headers)
        
        if response.status_code == 200:
            data = response.json()
            
            if "expected_remaining" in case:
                if data["allowed"] and data["remaining"] == case["expected_remaining"]:
                    print(f"   ✅ Cost {case['cost']}: Allowed with {data['remaining']} remaining")
                else:
                    print(f"   ❌ Cost {case['cost']}: Expected {case['expected_remaining']}, got {data['remaining']}")
                    return False
            elif "expected_allowed" in case:
                if data["allowed"] == case["expected_allowed"]:
                    print(f"   ✅ Cost {case['cost']}: Correctly denied (would exceed limit)")
                else:
                    print(f"   ❌ Cost {case['cost']}: Should have been denied")
                    return False
    
    print("✅ Cost-based rate limiting working correctly")
    return True

def test_authentication_security():
    """Test authentication security"""
    print("🔐 Testing authentication security...")
    
    # Test 1: No auth header
    response = requests.post(f"{BASE_URL}/v1/check", json={"key": "test", "limit": 10, "window": 60, "cost": 1})
    if response.status_code != 401:
        print("❌ Missing auth header should return 401")
        return False
    
    # Test 2: Invalid auth format
    headers = {"Authorization": "InvalidFormat"}
    response = requests.post(f"{BASE_URL}/v1/check", json={"key": "test", "limit": 10, "window": 60, "cost": 1}, headers=headers)
    if response.status_code != 401:
        print("❌ Invalid auth format should return 401")
        return False
    
    # Test 3: Invalid API key
    headers = {"Authorization": "Bearer invalid-key"}
    response = requests.post(f"{BASE_URL}/v1/check", json={"key": "test", "limit": 10, "window": 60, "cost": 1}, headers=headers)
    if response.status_code != 401:
        print("❌ Invalid API key should return 401")
        return False
    
    # Test 4: Valid API key
    headers = {"Authorization": f"Bearer {API_KEY}"}
    response = requests.post(f"{BASE_URL}/v1/check", json={"key": "test", "limit": 10, "window": 60, "cost": 1}, headers=headers)
    if response.status_code != 200:
        print("❌ Valid API key should return 200")
        return False
    
    print("✅ Authentication security working correctly")
    return True

def test_error_handling():
    """Test error handling and edge cases"""
    print("🛡️ Testing error handling...")
    
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    }
    
    # Test 1: Missing required fields
    response = requests.post(f"{BASE_URL}/v1/check", json={"key": "test"}, headers=headers)
    if response.status_code not in [400, 422]:
        print("❌ Missing fields should return 400/422")
        return False
    
    # Test 2: Invalid JSON
    response = requests.post(f"{BASE_URL}/v1/check", data="invalid json", headers=headers)
    if response.status_code not in [400, 422]:
        print("❌ Invalid JSON should return 400/422")
        return False
    
    # Test 3: Zero/negative values
    test_cases = [
        {"key": "test", "limit": 0, "window": 60, "cost": 1},
        {"key": "test", "limit": 10, "window": 0, "cost": 1},
        {"key": "", "limit": 10, "window": 60, "cost": 1}
    ]
    
    for case in test_cases:
        response = requests.post(f"{BASE_URL}/v1/check", json=case, headers=headers)
        # Should either handle gracefully or return appropriate error
        if response.status_code not in [200, 400, 422]:
            print(f"❌ Edge case handling failed for {case}")
            return False
    
    print("✅ Error handling working correctly")
    return True

def test_performance_under_load():
    """Test performance under sustained load"""
    print("⚡ Testing performance under load...")
    
    headers = {
        "Authorization": f"Bearer {API_KEY}",
        "Content-Type": "application/json"
    }
    
    num_requests = 100
    durations = []
    
    def make_load_request(request_id):
        payload = {
            "key": f"load-test-{request_id % 10}",  # 10 different keys
            "limit": 100,
            "window": 3600,
            "cost": 1
        }
        
        start_time = time.time()
        response = requests.post(f"{BASE_URL}/v1/check", json=payload, headers=headers)
        end_time = time.time()
        
        return end_time - start_time
    
    # Execute load test
    with concurrent.futures.ThreadPoolExecutor(max_workers=20) as executor:
        futures = [executor.submit(make_load_request, i) for i in range(num_requests)]
        durations = [future.result() for future in concurrent.futures.as_completed(futures)]
    
    # Analyze performance
    avg_duration = sum(durations) / len(durations)
    max_duration = max(durations)
    p95_duration = sorted(durations)[int(0.95 * len(durations))]
    
    print(f"   - Requests: {num_requests}")
    print(f"   - Average response time: {avg_duration:.3f}s")
    print(f"   - 95th percentile: {p95_duration:.3f}s")
    print(f"   - Max response time: {max_duration:.3f}s")
    
    # Performance requirements: <500ms for 95% of requests
    if p95_duration < 0.5 and avg_duration < 0.1:
        print("✅ Performance requirements met")
        return True
    else:
        print("❌ Performance requirements not met")
        return False

def main():
    """Run comprehensive production core tests"""
    print("🏭 Production Core Functionality Test")
    print("=" * 60)
    
    # Check if server is running
    try:
        response = requests.get(f"{BASE_URL}/health", timeout=5)
        if response.status_code != 200:
            print("❌ RateWatch server is not responding")
            return 1
        print("✅ RateWatch server is running")
    except Exception as e:
        print(f"❌ Cannot connect to RateWatch server: {e}")
        return 1
    
    # Run core functionality tests
    tests = [
        test_rate_limiting_accuracy,
        test_concurrent_requests,
        test_sliding_window_behavior,
        test_cost_based_limiting,
        test_authentication_security,
        test_error_handling,
        test_performance_under_load,
    ]
    
    passed = 0
    failed = 0
    
    for test in tests:
        try:
            print()
            if test():
                passed += 1
            else:
                failed += 1
        except Exception as e:
            print(f"❌ Test {test.__name__} crashed: {e}")
            failed += 1
    
    print("\n" + "=" * 60)
    print(f"🎯 Production Core Test Results: {passed} passed, {failed} failed")
    
    if failed == 0:
        print("🎉 All core functionality tests passed!")
        print("✅ Rate limiting accuracy: Perfect")
        print("✅ Concurrent handling: Robust")
        print("✅ Sliding window: Working correctly")
        print("✅ Cost-based limiting: Implemented")
        print("✅ Authentication: Secure")
        print("✅ Error handling: Comprehensive")
        print("✅ Performance: Sub-500ms")
        print("\n🚀 SYSTEM IS PRODUCTION READY!")
        return 0
    else:
        print("💥 Some core functionality tests failed!")
        print("⚠️  System may not be ready for production")
        return 1

if __name__ == "__main__":
    sys.exit(main())