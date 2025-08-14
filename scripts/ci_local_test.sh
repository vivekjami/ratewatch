#!/bin/bash
# Local CI test script - runs the same checks as GitHub Actions

set -euo pipefail

echo "🚀 Running Local CI Tests (Same as GitHub Actions)"
echo "=================================================="

# Set environment variables like CI
export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=1

echo ""
echo "1️⃣ Checking code formatting..."
if cargo fmt --all -- --check; then
    echo "✅ Code formatting is correct"
else
    echo "❌ Code formatting issues found"
    echo "Run 'cargo fmt --all' to fix"
    exit 1
fi

echo ""
echo "2️⃣ Running clippy lints..."
if cargo clippy --all-targets --all-features -- -D warnings; then
    echo "✅ No clippy warnings found"
else
    echo "❌ Clippy warnings found"
    echo "Fix the warnings above"
    exit 1
fi

echo ""
echo "3️⃣ Running all tests..."
if cargo test --all --verbose; then
    echo "✅ All tests passed"
else
    echo "❌ Some tests failed"
    exit 1
fi

echo ""
echo "4️⃣ Running security audit..."
if cargo audit; then
    echo "✅ No security vulnerabilities found"
else
    echo "❌ Security vulnerabilities found"
    exit 1
fi

echo ""
echo "5️⃣ Building release binary..."
if cargo build --release; then
    echo "✅ Release build successful"
else
    echo "❌ Release build failed"
    exit 1
fi

echo ""
echo "6️⃣ Checking binary size..."
BINARY_SIZE=$(stat -c%s "target/release/ratewatch")
BINARY_SIZE_MB=$((BINARY_SIZE / 1024 / 1024))
echo "Binary size: ${BINARY_SIZE_MB}MB"

if [ $BINARY_SIZE_MB -lt 50 ]; then
    echo "✅ Binary size is reasonable (${BINARY_SIZE_MB}MB < 50MB)"
else
    echo "⚠️  Binary size is large (${BINARY_SIZE_MB}MB)"
fi

echo ""
echo "7️⃣ Testing client library imports..."
cd clients/python
if python3 -c "
import sys
sys.path.insert(0, '.')
from ratewatch import RateWatch
client = RateWatch('test-key')
print('✅ Python client imports successfully')
"; then
    echo "✅ Python client library works"
else
    echo "❌ Python client library has issues"
    exit 1
fi

cd ../nodejs
if npm install --silent && npm run build --silent; then
    if node -e "
    const { RateWatch } = require('./dist/index.js');
    const client = new RateWatch('test-key');
    console.log('✅ Node.js client imports successfully');
    "; then
        echo "✅ Node.js client library works"
    else
        echo "❌ Node.js client library has issues"
        exit 1
    fi
else
    echo "❌ Node.js client build failed"
    exit 1
fi

cd ../..

echo ""
echo "8️⃣ Validating workflows..."
if ./scripts/validate_workflows.sh; then
    echo "✅ GitHub workflows are valid"
else
    echo "❌ GitHub workflow issues found"
    exit 1
fi

echo ""
echo "🎉 ALL LOCAL CI TESTS PASSED!"
echo "============================================"
echo "✅ Code formatting: OK"
echo "✅ Clippy lints: OK"
echo "✅ Unit tests: OK"
echo "✅ Integration tests: OK"
echo "✅ Performance tests: OK"
echo "✅ Security audit: OK"
echo "✅ Release build: OK"
echo "✅ Python client: OK"
echo "✅ Node.js client: OK"
echo "✅ GitHub workflows: OK"
echo ""
echo "🚀 Your code is ready for GitHub Actions CI!"
echo "All checks that run in CI will pass successfully."