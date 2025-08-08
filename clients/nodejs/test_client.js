#!/usr/bin/env node

/**
 * Test script for RateWatch Node.js client library
 * Tests both basic functionality and GDPR compliance features
 */

const { RateWatch, RateLimitExceededError, AuthenticationError } = require('./dist/index.js');

async function testBasicFunctionality() {
  console.log('🧪 Testing basic rate limiting functionality...');
  
  // Use a test API key (should be at least 32 characters)
  const apiKey = 'test-api-key-12345678901234567890123';
  const client = new RateWatch(apiKey, 'http://localhost:8081');
  
  try {
    // Test rate limit check
    const result = await client.check(
      'test:nodejs:user123',
      5,
      60, // 5 requests per minute
      1
    );
    
    console.log('✅ Rate limit check successful:');
    console.log(`   - Allowed: ${result.allowed}`);
    console.log(`   - Remaining: ${result.remaining}`);
    console.log(`   - Reset in: ${result.resetIn}s`);
    if (result.retryAfter) {
      console.log(`   - Retry after: ${result.retryAfter}s`);
    }
    
    return true;
    
  } catch (error) {
    console.log(`❌ Rate limit check failed: ${error.message}`);
    return false;
  }
}

async function testRateLimitExhaustion() {
  console.log('\n🧪 Testing rate limit exhaustion...');
  
  const apiKey = 'test-api-key-12345678901234567890123';
  const client = new RateWatch(apiKey, 'http://localhost:8081');
  
  try {
    // Make multiple requests to exhaust the limit
    for (let i = 0; i < 6; i++) { // More than the limit of 5
      const result = await client.check(
        'test:nodejs:exhaust',
        5,
        60,
        1
      );
      
      console.log(`   Request ${i+1}: allowed=${result.allowed}, remaining=${result.remaining}`);
      
      if (!result.allowed) {
        console.log(`✅ Rate limit properly enforced after ${i+1} requests`);
        console.log(`   - Retry after: ${result.retryAfter}s`);
        return true;
      }
    }
    
    console.log('❌ Rate limit was not enforced as expected');
    return false;
    
  } catch (error) {
    console.log(`❌ Rate limit exhaustion test failed: ${error.message}`);
    return false;
  }
}

async function testEnhancedClient() {
  console.log('\n🧪 Testing enhanced client with exceptions...');
  
  const apiKey = 'test-api-key-12345678901234567890123';
  const client = new RateWatch(apiKey, 'http://localhost:8081');
  
  try {
    // Use a unique key for this test
    const testKey = `test:nodejs:enhanced:${Date.now()}`;
    
    // First request should succeed
    const result = await client.checkWithExceptions(
      testKey,
      2,
      60,
      1
    );
    console.log(`✅ First request allowed: ${result.remaining} remaining`);
    
    // Second request should also succeed but use up the limit
    await client.checkWithExceptions(testKey, 2, 60, 1);
    
    // Third request should raise an exception
    try {
      await client.checkWithExceptions(testKey, 2, 60, 1);
      console.log('❌ Exception was not raised when rate limit exceeded');
      return false;
    } catch (error) {
      if (error instanceof RateLimitExceededError) {
        console.log(`✅ RateLimitExceededError exception properly raised: ${error.message}`);
        console.log(`   - Retry after: ${error.retryAfter}s`);
        return true;
      } else {
        console.log(`❌ Unexpected exception type: ${error.constructor.name}`);
        return false;
      }
    }
    
  } catch (error) {
    console.log(`❌ Enhanced client test failed: ${error.message}`);
    return false;
  }
}

async function testGdprCompliance() {
  console.log('\n🧪 Testing GDPR compliance features...');
  
  const apiKey = 'test-api-key-12345678901234567890123';
  const client = new RateWatch(apiKey, 'http://localhost:8081');
  
  try {
    const userId = 'test-user-nodejs-123';
    
    // Create some data for the user
    await client.check(`user:${userId}:api`, 10, 3600, 1);
    await client.check(`user:${userId}:upload`, 5, 3600, 1);
    
    // Get user data summary
    const summary = await client.getUserDataSummary(userId);
    console.log('✅ User data summary retrieved:');
    console.log(`   - User ID: ${summary.user_id || 'N/A'}`);
    console.log(`   - Keys count: ${summary.keys_count || 0}`);
    console.log(`   - Data types: ${summary.data_types || []}`);
    
    // Delete user data
    const success = await client.deleteUserData(userId, 'test_cleanup');
    if (success) {
      console.log('✅ User data deletion successful');
    } else {
      console.log('⚠️  User data deletion returned false (may be expected)');
    }
    
    return true;
    
  } catch (error) {
    console.log(`❌ GDPR compliance test failed: ${error.message}`);
    return false;
  }
}

async function testHealthChecks() {
  console.log('\n🧪 Testing health check endpoints...');
  
  const apiKey = 'test-api-key-12345678901234567890123';
  const client = new RateWatch(apiKey, 'http://localhost:8081');
  
  try {
    // Basic health check
    const health = await client.healthCheck();
    console.log('✅ Basic health check:');
    console.log(`   - Status: ${health.status || 'unknown'}`);
    console.log(`   - Timestamp: ${health.timestamp || 'N/A'}`);
    
    // Detailed health check
    const detailed = await client.detailedHealthCheck();
    console.log('✅ Detailed health check:');
    console.log(`   - Status: ${detailed.status || 'unknown'}`);
    if (detailed.dependencies) {
      const deps = detailed.dependencies;
      if (deps.redis) {
        const redisStatus = deps.redis;
        console.log(`   - Redis status: ${redisStatus.status || 'unknown'}`);
        if (redisStatus.latency_ms !== undefined) {
          console.log(`   - Redis latency: ${redisStatus.latency_ms}ms`);
        }
      }
    }
    
    return true;
    
  } catch (error) {
    console.log(`❌ Health check test failed: ${error.message}`);
    return false;
  }
}

async function testAuthenticationError() {
  console.log('\n🧪 Testing authentication error handling...');
  
  // Use an invalid API key
  const invalidClient = new RateWatch('invalid-key', 'http://localhost:8081');
  
  try {
    await invalidClient.checkWithExceptions('test:auth', 10, 60, 1);
    console.log('❌ Authentication error was not raised with invalid API key');
    return false;
  } catch (error) {
    if (error instanceof AuthenticationError) {
      console.log('✅ Authentication error properly raised with invalid API key');
      return true;
    } else {
      console.log(`❌ Unexpected error with invalid API key: ${error.message}`);
      return false;
    }
  }
}

async function main() {
  console.log('🚀 Starting RateWatch Node.js Client Tests');
  console.log('='.repeat(50));
  
  // Check if server is running
  const apiKey = 'test-api-key-12345678901234567890123';
  const client = new RateWatch(apiKey, 'http://localhost:8081');
  
  try {
    const health = await client.healthCheck();
    console.log(`✅ RateWatch server is running (status: ${health.status || 'unknown'})`);
  } catch (error) {
    console.log(`❌ Cannot connect to RateWatch server: ${error.message}`);
    console.log('Please make sure the RateWatch server is running on http://localhost:8081');
    process.exit(1);
  }
  
  // Run tests
  const tests = [
    testBasicFunctionality,
    testRateLimitExhaustion,
    testEnhancedClient,
    testGdprCompliance,
    testHealthChecks,
    testAuthenticationError,
  ];
  
  let passed = 0;
  let failed = 0;
  
  for (const test of tests) {
    try {
      if (await test()) {
        passed++;
      } else {
        failed++;
      }
    } catch (error) {
      console.log(`❌ Test ${test.name} crashed: ${error.message}`);
      failed++;
    }
  }
  
  console.log('\n' + '='.repeat(50));
  console.log(`🎯 Test Results: ${passed} passed, ${failed} failed`);
  
  if (failed === 0) {
    console.log('🎉 All tests passed! Node.js client is working correctly.');
    process.exit(0);
  } else {
    console.log('💥 Some tests failed. Please check the output above.');
    process.exit(1);
  }
}

// Handle unhandled promise rejections
process.on('unhandledRejection', (reason, promise) => {
  console.error('Unhandled Rejection at:', promise, 'reason:', reason);
  process.exit(1);
});

if (require.main === module) {
  main();
}
