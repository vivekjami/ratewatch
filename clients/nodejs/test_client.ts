#!/usr/bin/env node
/**
 * Simple test script to validate the Node.js client library works correctly.
 * Run this with: npx ts-node test_client.ts
 */

import { RateWatch, RateLimitExceededError, AuthenticationError } from './src/index';

async function testNodejsClient(): Promise<boolean> {
    console.log('Testing Node.js RateWatch client...');
    
    // Test with localhost (assuming server is running)
    const client = new RateWatch(
        'rw_1234567890abcdef1234567890abcdef',
        'http://localhost:8081'
    );
    
    try {
        // Test health check
        const health = await client.healthCheck();
        console.log(`✓ Health check: ${health.status}`);
        
        // Test rate limiting
        const result = await client.check('test:nodejs:client', 5, 60, 1);
        console.log(`✓ Rate limit check: allowed=${result.allowed}, remaining=${result.remaining}`);
        
        // Test enhanced client with exceptions
        try {
            const result1 = await client.checkWithExceptions('test:nodejs:enhanced', 1, 60, 1);
            console.log(`✓ Enhanced client: allowed=${result1.allowed}`);
            
            // This should raise an exception
            await client.checkWithExceptions('test:nodejs:enhanced', 1, 60, 1);
            console.log('✗ Should have raised RateLimitExceededError');
            return false;
        } catch (error) {
            if (error instanceof RateLimitExceededError) {
                console.log(`✓ Rate limit exception caught: ${error.message}`);
            } else {
                throw error;
            }
        }
        
        // Test GDPR features
        const deleteResult = await client.deleteUserData('test:user:123');
        console.log(`✓ GDPR delete: success=${deleteResult}`);
        
        console.log('✓ Node.js client tests passed!');
        return true;
        
    } catch (error) {
        console.error(`✗ Node.js client test failed: ${error}`);
        console.log('Note: Make sure the RateWatch server is running on localhost:8081');
        return false;
    }
}

// Run the test
testNodejsClient().then(success => {
    process.exit(success ? 0 : 1);
}).catch(error => {
    console.error('Test runner failed:', error);
    process.exit(1);
});