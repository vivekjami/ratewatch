#!/bin/bash
# Generate a secure API key for RateWatch

set -euo pipefail

# Generate a random 32-character API key
API_KEY="rw_$(openssl rand -hex 24)"

echo "Generated API Key: $API_KEY"
echo ""
echo "Add this to your environment:"
echo "export RATEWATCH_API_KEY=\"$API_KEY\""
echo ""
echo "Or add to your .env file:"
echo "API_KEY_SECRET=$API_KEY"
echo ""
echo "⚠️  Keep this key secure and never commit it to version control!"