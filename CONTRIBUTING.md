# Contributing to RateWatch

Thank you for your interest in contributing to RateWatch! This document provides guidelines and information for contributors.

## Code of Conduct

This project adheres to a code of conduct. By participating, you are expected to uphold this code.

## Getting Started

### Prerequisites

- Rust 1.70 or later
- Redis 6.0 or later
- Docker and Docker Compose (for testing)
- Git

### Development Setup

1. Fork and clone the repository:
   ```bash
   git clone https://github.com/your-username/ratewatch.git
   cd ratewatch
   ```

2. Install Rust if you haven't already:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

3. Start Redis for testing:
   ```bash
   docker-compose up -d redis
   ```

4. Run the test suite:
   ```bash
   cargo test --all
   ```

5. Start the development server:
   ```bash
   cargo run
   ```

## Development Workflow

### Branch Naming

- `feature/description` - New features
- `fix/description` - Bug fixes
- `docs/description` - Documentation updates
- `refactor/description` - Code refactoring
- `test/description` - Test improvements

### Commit Messages

Follow conventional commit format:
- `feat: add new rate limiting algorithm`
- `fix: resolve memory leak in Redis connection`
- `docs: update API documentation`
- `test: add integration tests for GDPR endpoints`
- `refactor: simplify authentication middleware`

### Pull Request Process

1. Create a feature branch from `main`
2. Make your changes with appropriate tests
3. Ensure all tests pass: `cargo test --all`
4. Run formatting: `cargo fmt --all`
5. Run linting: `cargo clippy --all-targets --all-features -- -D warnings`
6. Run security audit: `cargo audit`
7. Update documentation if needed
8. Submit a pull request with a clear description

## Code Standards

### Rust Code Style

- Follow standard Rust formatting (`cargo fmt`)
- Use `cargo clippy` and address all warnings
- Write comprehensive tests for new functionality
- Add documentation comments for public APIs
- Use meaningful variable and function names
- Handle errors appropriately (don't use `unwrap()` in production code)

### Testing Requirements

- Unit tests for all core functionality
- Integration tests for API endpoints
- Performance tests for critical paths
- Security tests for authentication and input validation
- All tests must pass before merging

### Documentation

- Update README.md for user-facing changes
- Add inline documentation for complex code
- Update API documentation for endpoint changes
- Include examples in documentation

## Security

### Reporting Security Issues

Please report security vulnerabilities to security@ratewatch.dev. Do not create public GitHub issues for security problems.

### Security Guidelines

- Never commit secrets or API keys
- Validate all user inputs
- Use secure defaults
- Follow OWASP security guidelines
- Implement proper authentication and authorization

## Performance

### Performance Requirements

- Rate limit checks must complete in <500ms
- Memory usage should remain stable under load
- CPU usage should be optimized for high throughput
- Redis operations should be efficient and batched when possible

### Benchmarking

Run performance tests before submitting changes:
```bash
cargo test --release performance_tests
```

## Architecture

### Core Components

- `rate_limiter.rs` - Core rate limiting logic with Redis
- `auth.rs` - API key authentication and validation
- `api.rs` - HTTP API endpoints and routing
- `privacy.rs` - GDPR compliance features
- `analytics.rs` - Usage analytics and logging
- `metrics.rs` - Prometheus metrics collection

### Design Principles

- Security by default
- Performance first
- GDPR compliance built-in
- Comprehensive error handling
- Extensive testing
- Clear documentation

## Client Libraries

### Python Client

Located in `clients/python/`:
- Follow PEP 8 style guidelines
- Include type hints
- Write comprehensive tests
- Update documentation

### Node.js Client

Located in `clients/nodejs/`:
- Use TypeScript for type safety
- Follow ESLint configuration
- Include proper error handling
- Write comprehensive tests

## Release Process

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Create a git tag: `git tag v1.x.x`
4. Push tag: `git push origin v1.x.x`
5. GitHub Actions will automatically build and release

## Getting Help

- Check existing issues and discussions
- Join our Discord community
- Read the documentation
- Ask questions in GitHub Discussions

## Recognition

Contributors will be recognized in:
- CONTRIBUTORS.md file
- Release notes
- Project documentation

Thank you for contributing to RateWatch!