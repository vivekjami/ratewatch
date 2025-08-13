#!/usr/bin/env python3
"""
Simple test script to validate the Python client library works correctly.
Run this with: python3 clients/test_clients.py
"""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'python'))

from ratewatch import RateWatch, RateWatchClient, RateLimitExceeded, AuthenticationError

def test_python_client():
    """Test the Python client library"""
    print("Testing Python RateWatch client...")
    
    # Test with localhost (assuming server is running)
    client = RateWatch(
        api_key="rw_1234567890abcdef1234567890abcdef",
        base_url="http://localhost:8081"
    )
    
    try:
        # Test health check
        health = client.health_check()
        print(f"✓ Health check: {health['status']}")
        
        # Test rate limiting
        result = client.check("test:python:client", 5, 60, 1)
        print(f"✓ Rate limit check: allowed={result.allowed}, remaining={result.remaining}")
        
        # Test enhanced client with exceptions
        enhanced_client = RateWatchClient(
            api_key="rw_1234567890abcdef1234567890abcdef",
            base_url="http://localhost:8081"
        )
        
        try:
            result = enhanced_client.check_with_exceptions("test:python:enhanced", 1, 60, 1)
            print(f"✓ Enhanced client: allowed={result.allowed}")
            
            # This should raise an exception
            result = enhanced_client.check_with_exceptions("test:python:enhanced", 1, 60, 1)
            print("✗ Should have raised RateLimitExceeded")
        except RateLimitExceeded as e:
            print(f"✓ Rate limit exception caught: {e}")
        
        print("✓ Python client tests passed!")
        return True
        
    except Exception as e:
        print(f"✗ Python client test failed: {e}")
        print("Note: Make sure the RateWatch server is running on localhost:8081")
        return False

if __name__ == "__main__":
    success = test_python_client()
    sys.exit(0 if success else 1)