import axios, { AxiosInstance, AxiosResponse } from 'axios';

export interface RateLimitResult {
  allowed: boolean;
  remaining: number;
  resetIn: number;
  retryAfter?: number;
}

export interface RateLimitRequest {
  key: string;
  limit: number;
  window: number;
  cost?: number;
}

export interface UserDataSummary {
  user_id: string;
  keys_count: number;
  total_requests: number;
  data_types: string[];
}

export interface HealthStatus {
  status: string;
  timestamp: string;
  dependencies?: {
    redis: {
      status: string;
      latency_ms?: number;
    };
  };
}

export class RateWatchError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'RateWatchError';
  }
}

export class RateLimitExceededError extends RateWatchError {
  public retryAfter?: number;

  constructor(message: string, retryAfter?: number) {
    super(message);
    this.name = 'RateLimitExceededError';
    this.retryAfter = retryAfter;
  }
}

export class AuthenticationError extends RateWatchError {
  constructor(message: string = 'Invalid API key') {
    super(message);
    this.name = 'AuthenticationError';
  }
}

export class RateWatch {
  private client: AxiosInstance;
  private apiKey: string;
  private baseUrl: string;

  constructor(apiKey: string, baseUrl: string = 'http://localhost:8081') {
    this.apiKey = apiKey;
    this.baseUrl = baseUrl;
    
    this.client = axios.create({
      baseURL: baseUrl,
      headers: {
        'Authorization': `Bearer ${apiKey}`,
        'Content-Type': 'application/json',
      },
      timeout: 10000, // 10 second timeout
    });
  }

  /**
   * Check rate limit for a given key
   */
  async check(
    key: string,
    limit: number,
    window: number,
    cost: number = 1
  ): Promise<RateLimitResult> {
    const payload: RateLimitRequest = {
      key,
      limit,
      window,
      cost,
    };

    try {
      const response: AxiosResponse = await this.client.post('/v1/check', payload);
      
      return {
        allowed: response.data.allowed,
        remaining: response.data.remaining,
        resetIn: response.data.reset_in,
        retryAfter: response.data.retry_after,
      };
    } catch (error) {
      this.handleError(error);
      throw error; // This should never be reached due to handleError throwing
    }
  }

  /**
   * Delete all data for a user (GDPR compliance)
   */
  async deleteUserData(userId: string, reason: string = 'user_request'): Promise<boolean> {
    const payload = { user_id: userId, reason };

    try {
      const response: AxiosResponse = await this.client.post('/v1/privacy/delete', payload);
      return response.data.success || false;
    } catch (error) {
      return false;
    }
  }

  /**
   * Get summary of user data (GDPR compliance)
   */
  async getUserDataSummary(userId: string): Promise<UserDataSummary> {
    const payload = { user_id: userId };

    try {
      const response: AxiosResponse = await this.client.post('/v1/privacy/summary', payload);
      return response.data;
    } catch (error) {
      this.handleError(error);
      throw error; // This should never be reached due to handleError throwing
    }
  }

  /**
   * Check service health
   */
  async healthCheck(): Promise<HealthStatus> {
    try {
      const response: AxiosResponse = await this.client.get('/health');
      return response.data;
    } catch (error) {
      this.handleError(error);
      throw error; // This should never be reached due to handleError throwing
    }
  }

  /**
   * Get detailed health information including dependencies
   */
  async detailedHealthCheck(): Promise<HealthStatus> {
    try {
      const response: AxiosResponse = await this.client.get('/health/detailed');
      return response.data;
    } catch (error) {
      this.handleError(error);
      throw error; // This should never be reached due to handleError throwing
    }
  }

  /**
   * Check rate limit and throw exceptions for common error cases
   */
  async checkWithExceptions(
    key: string,
    limit: number,
    window: number,
    cost: number = 1
  ): Promise<RateLimitResult> {
    const result = await this.check(key, limit, window, cost);
    
    if (!result.allowed) {
      throw new RateLimitExceededError(
        `Rate limit exceeded for key '${key}'. Try again in ${result.retryAfter}s`,
        result.retryAfter
      );
    }
    
    return result;
  }

  private handleError(error: any): never {
    if (error.response) {
      const status = error.response.status;
      const message = error.response.data?.message || error.message;

      if (status === 401) {
        throw new AuthenticationError('Invalid API key');
      } else if (status >= 400 && status < 500) {
        throw new RateWatchError(`Client error: ${message}`);
      } else if (status >= 500) {
        throw new RateWatchError(`Server error: ${message}`);
      }
    } else if (error.request) {
      throw new RateWatchError('Network error: Unable to reach the server');
    }
    
    throw new RateWatchError(`Request failed: ${error.message}`);
  }
}

// Export default instance
export default RateWatch;
