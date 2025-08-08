import requests
import json
from typing import Dict, Optional
from dataclasses import dataclass

@dataclass
class RateLimitResult:
    allowed: bool
    remaining: int
    reset_in: int
    retry_after: Optional[int] = None

class RateWatch:
    def __init__(self, api_key: str, base_url: str = "http://localhost:8081"):
        self.api_key = api_key
        self.base_url = base_url
        self.session = requests.Session()
        self.session.headers.update({
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json"
        })
    
    def check(self, key: str, limit: int, window: int, cost: int = 1) -> RateLimitResult:
        """Check rate limit for a given key"""
        payload = {
            "key": key,
            "limit": limit,
            "window": window,
            "cost": cost
        }
        
        response = self.session.post(f"{self.base_url}/v1/check", json=payload)
        response.raise_for_status()
        
        data = response.json()
        return RateLimitResult(
            allowed=data["allowed"],
            remaining=data["remaining"],
            reset_in=data["reset_in"],
            retry_after=data.get("retry_after")
        )
    
    def delete_user_data(self, user_id: str, reason: str = "user_request") -> bool:
        """Delete all data for a user (GDPR compliance)"""
        payload = {"user_id": user_id, "reason": reason}
        try:
            response = self.session.post(f"{self.base_url}/v1/privacy/delete", json=payload)
            response.raise_for_status()
            return response.json().get("success", False)
        except requests.RequestException:
            return False
    
    def get_user_data_summary(self, user_id: str) -> Dict:
        """Get summary of user data (GDPR compliance)"""
        payload = {"user_id": user_id}
        response = self.session.post(f"{self.base_url}/v1/privacy/summary", json=payload)
        response.raise_for_status()
        return response.json()
    
    def health_check(self) -> Dict:
        """Check service health"""
        response = self.session.get(f"{self.base_url}/health")
        response.raise_for_status()
        return response.json()
    
    def detailed_health_check(self) -> Dict:
        """Get detailed health information including dependencies"""
        response = self.session.get(f"{self.base_url}/health/detailed")
        response.raise_for_status()
        return response.json()

# Exception classes for better error handling
class RateWatchError(Exception):
    """Base exception for RateWatch client errors"""
    pass

class RateLimitExceeded(RateWatchError):
    """Raised when rate limit is exceeded"""
    def __init__(self, message: str, retry_after: Optional[int] = None):
        super().__init__(message)
        self.retry_after = retry_after

class AuthenticationError(RateWatchError):
    """Raised when API key authentication fails"""
    pass

class RateWatchClient(RateWatch):
    """Enhanced client with better error handling and convenience methods"""
    
    def check_with_exceptions(self, key: str, limit: int, window: int, cost: int = 1) -> RateLimitResult:
        """Check rate limit and raise exceptions for common error cases"""
        try:
            result = self.check(key, limit, window, cost)
            if not result.allowed:
                raise RateLimitExceeded(
                    f"Rate limit exceeded for key '{key}'. Try again in {result.retry_after}s",
                    retry_after=result.retry_after
                )
            return result
        except requests.HTTPError as e:
            if e.response.status_code == 401:
                raise AuthenticationError("Invalid API key")
            raise RateWatchError(f"HTTP error: {e}")
        except requests.RequestException as e:
            raise RateWatchError(f"Request failed: {e}")
