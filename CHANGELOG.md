# Changelog

All notable changes to RateWatch will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-08-13

### Added
- Initial production release of RateWatch
- High-performance rate limiting with Redis sliding window algorithm
- Sub-500ms response time guarantee
- API key authentication with Blake3 hashing
- GDPR compliance features (data deletion and summaries)
- Comprehensive security headers and CORS support
- Prometheus metrics endpoint for monitoring
- Health check endpoints (basic and detailed)
- Docker and Docker Compose deployment support
- Python client library with full type hints
- Node.js/TypeScript client library with proper types
- Comprehensive test suite (unit, integration, performance)
- Production-ready logging and error handling
- Automatic data expiration for GDPR compliance
- Cost-based rate limiting support
- Analytics and activity logging
- Privacy management endpoints

### Security
- Zero known vulnerabilities in dependencies
- Secure API key validation (minimum 32 characters)
- Protection against timing attacks
- Input validation and sanitization
- Docker security best practices (non-root user, read-only filesystem)
- Security headers (HSTS, X-Frame-Options, etc.)

### Performance
- Sub-500ms response time for rate limit checks
- Efficient Redis connection pooling
- Optimized binary size with release profile
- Memory-efficient sliding window implementation
- Concurrent request handling without degradation

### Documentation
- Comprehensive README with quick start guide
- API reference documentation
- Client library documentation (Python and Node.js)
- Docker deployment instructions
- Production validation summary

[1.0.0]: https://github.com/ratewatch/ratewatch/releases/tag/v1.0.0