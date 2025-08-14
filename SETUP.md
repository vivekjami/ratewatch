# RateWatch Setup Guide

## Quick Start

### Prerequisites
- Rust 1.82+ 
- Redis 7+
- Docker (optional)

### Local Development Setup

1. **Clone the repository**
```bash
git clone <repository-url>
cd ratewatch
```

2. **Install dependencies**
```bash
cargo build
```

3. **Start Redis**
```bash
# Using Docker
docker run -d --name redis -p 6379:6379 redis:7-alpine

# Or install locally
sudo apt-get install redis-server
redis-server
```

4. **Configure environment**
```bash
cp .env.example .env
# Edit .env with your configuration
```

5. **Run the application**
```bash
cargo run
```

6. **Test the setup**
```bash
curl http://localhost:8081/health
```

## Production Setup

See [DEPLOYMENT.md](docs/DEPLOYMENT.md) for production deployment instructions.

## API Documentation

See [API.md](docs/API.md) for complete API reference.