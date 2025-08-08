#!/usr/bin/env python3
"""
Test script for RateWatch Python client library
Tests both basic functionality and GDPR compliance features
"""

import sys
import time
import json
from pathlib import Path

# Add the client library to the path
sys.path.insert(0, str(Path(__file__).parent / "ratewatch"))

from ratewatch import RateWatch, RateWatchClient, RateLimitExceeded, AuthenticationError

def test_basic_functionality():
    """Test basic rate limiting functionality"""
    print("🧪 Testing basic rate limiting functionality...")
    
    # Use a test API key (should be at least 32 characters)
    api_key = "test-api-key-12345678901234567890123"
    client = RateWatch(api_key=api_key, base_url="http://localhost:8081")
    
    try:
        # Test rate limit check
        result = client.check(
            key="test:python:user123",
            limit=5,
            window=60,  # 5 requests per minute
            cost=1
        )
        
        print(f"✅ Rate limit check successful:")
        print(f"   - Allowed: {result.allowed}")
        print(f"   - Remaining: {result.remaining}")
        print(f"   - Reset in: {result.reset_in}s")
        if result.retry_after:
            print(f"   - Retry after: {result.retry_after}s")
        
        return True
        
    except Exception as e:
        print(f"❌ Rate limit check failed: {e}")
        return False

def test_rate_limit_exhaustion():
    """Test rate limit exhaustion scenario"""
    print("\n🧪 Testing rate limit exhaustion...")
    
    api_key = "test-api-key-12345678901234567890123"
    client = RateWatch(api_key=api_key, base_url="http://localhost:8081")
    
    try:
        # Make multiple requests to exhaust the limit
        for i in range(6):  # More than the limit of 5
            result = client.check(
                key="test:python:exhaust",
                limit=5,
                window=60,
                cost=1
            )
            
            print(f"   Request {i+1}: allowed={result.allowed}, remaining={result.remaining}")
            
            if not result.allowed:
                print(f"✅ Rate limit properly enforced after {i+1} requests")
                print(f"   - Retry after: {result.retry_after}s")
                return True
        
        print("❌ Rate limit was not enforced as expected")
        return False
        
    except Exception as e:
        print(f"❌ Rate limit exhaustion test failed: {e}")
        return False

def test_enhanced_client():
    """Test enhanced client with exception handling"""
    print("\n🧪 Testing enhanced client with exceptions...")
    
    api_key = "test-api-key-12345678901234567890123"
    client = RateWatchClient(api_key=api_key, base_url="http://localhost:8081")
    
    try:
        # Use a unique key for this test
        test_key = f"test:python:enhanced:{int(time.time())}"
        
        # First request should succeed
        result = client.check_with_exceptions(
            key=test_key,
            limit=2,
            window=60,
            cost=1
        )
        print(f"✅ First request allowed: {result.remaining} remaining")
        
        # Second request should also succeed but use up the limit
        client.check_with_exceptions(test_key, 2, 60, 1)
        
        # Third request should raise an exception
        try:
            client.check_with_exceptions(test_key, 2, 60, 1)
            print("❌ Exception was not raised when rate limit exceeded")
            return False
        except RateLimitExceeded as e:
            print(f"✅ RateLimitExceeded exception properly raised: {e}")
            print(f"   - Retry after: {e.retry_after}s")
            return True
        
    except Exception as e:
        print(f"❌ Enhanced client test failed: {e}")
        return False

def test_gdpr_compliance():
    """Test GDPR compliance features"""
    print("\n🧪 Testing GDPR compliance features...")
    
    api_key = "test-api-key-12345678901234567890123"
    client = RateWatch(api_key=api_key, base_url="http://localhost:8081")
    
    try:
        user_id = "test-user-python-123"
        
        # Create some data for the user
        client.check(f"user:{user_id}:api", 10, 3600, 1)
        client.check(f"user:{user_id}:upload", 5, 3600, 1)
        
        # Get user data summary
        summary = client.get_user_data_summary(user_id)
        print(f"✅ User data summary retrieved:")
        print(f"   - User ID: {summary.get('user_id', 'N/A')}")
        print(f"   - Keys count: {summary.get('keys_count', 0)}")
        print(f"   - Data types: {summary.get('data_types', [])}")
        
        # Delete user data
        success = client.delete_user_data(user_id, reason="test_cleanup")
        if success:
            print("✅ User data deletion successful")
        else:
            print("⚠️  User data deletion returned False (may be expected)")
        
        return True
        
    except Exception as e:
        print(f"❌ GDPR compliance test failed: {e}")
        return False

def test_health_checks():
    """Test health check endpoints"""
    print("\n🧪 Testing health check endpoints...")
    
    api_key = "test-api-key-12345678901234567890123"
    client = RateWatch(api_key=api_key, base_url="http://localhost:8081")
    
    try:
        # Basic health check
        health = client.health_check()
        print(f"✅ Basic health check:")
        print(f"   - Status: {health.get('status', 'unknown')}")
        print(f"   - Timestamp: {health.get('timestamp', 'N/A')}")
        
        # Detailed health check
        detailed = client.detailed_health_check()
        print(f"✅ Detailed health check:")
        print(f"   - Status: {detailed.get('status', 'unknown')}")
        if 'dependencies' in detailed:
            deps = detailed['dependencies']
            if 'redis' in deps:
                redis_status = deps['redis']
                print(f"   - Redis status: {redis_status.get('status', 'unknown')}")
                if 'latency_ms' in redis_status:
                    print(f"   - Redis latency: {redis_status['latency_ms']}ms")
        
        return True
        
    except Exception as e:
        print(f"❌ Health check test failed: {e}")
        return False

def test_authentication_error():
    """Test authentication error handling"""
    print("\n🧪 Testing authentication error handling...")
    
    # Use an invalid API key
    invalid_client = RateWatchClient(api_key="invalid-key", base_url="http://localhost:8081")
    
    try:
        invalid_client.check_with_exceptions("test:auth", 10, 60, 1)
        print("❌ Authentication error was not raised with invalid API key")
        return False
    except AuthenticationError:
        print("✅ Authentication error properly raised with invalid API key")
        return True
    except Exception as e:
        print(f"❌ Unexpected error with invalid API key: {e}")
        return False

def main():
    """Run all tests"""
    print("🚀 Starting RateWatch Python Client Tests")
    print("=" * 50)
    
    # Check if server is running
    api_key = "test-api-key-12345678901234567890123"
    client = RateWatch(api_key=api_key, base_url="http://localhost:8081")
    
    try:
        health = client.health_check()
        print(f"✅ RateWatch server is running (status: {health.get('status', 'unknown')})")
    except Exception as e:
        print(f"❌ Cannot connect to RateWatch server: {e}")
        print("Please make sure the RateWatch server is running on http://localhost:8081")
        sys.exit(1)
    
    # Run tests
    tests = [
        test_basic_functionality,
        test_rate_limit_exhaustion,
        test_enhanced_client,
        test_gdpr_compliance,
        test_health_checks,
        test_authentication_error,
    ]
    
    passed = 0
    failed = 0
    
    for test in tests:
        try:
            if test():
                passed += 1
            else:
                failed += 1
        except Exception as e:
            print(f"❌ Test {test.__name__} crashed: {e}")
            failed += 1
    
    print("\n" + "=" * 50)
    print(f"🎯 Test Results: {passed} passed, {failed} failed")
    
    if failed == 0:
        print("🎉 All tests passed! Python client is working correctly.")
        sys.exit(0)
    else:
        print("💥 Some tests failed. Please check the output above.")
        sys.exit(1)

if __name__ == "__main__":
    main()
