#!/bin/bash

# RateWatch API Key Generator
# This script generates secure API keys for RateWatch without any cost

# Colors for output
BOLD="\e[1m"
GREEN="\e[32m"
YELLOW="\e[33m"
RESET="\e[0m"

echo -e "${BOLD}🔑 RateWatch API Key Generator${RESET}"
echo "=============================="
echo

# Function to generate a key
generate_key() {
  local prefix=$1
  local timestamp=$(date +%s)
  local random=$(openssl rand -hex 16)
  echo "${prefix}_${timestamp}_${random}"
}

# Parse arguments
KEY_NAME=$1
OUTPUT_FILE=$2

if [ -z "$KEY_NAME" ]; then
  echo -e "${YELLOW}No key name specified. Using 'default'.${RESET}"
  KEY_NAME="default"
fi

if [ -z "$OUTPUT_FILE" ]; then
  OUTPUT_FILE="api_key_${KEY_NAME}.txt"
fi

# Generate the key
echo -e "${BOLD}Generating API key for: ${GREEN}${KEY_NAME}${RESET}"
API_KEY=$(generate_key "rw")

# Save the key
echo -n "$API_KEY" > "$OUTPUT_FILE"
chmod 600 "$OUTPUT_FILE"

echo -e "${GREEN}✅ API key generated successfully!${RESET}"
echo
echo -e "Key: ${BOLD}${API_KEY}${RESET}"
echo -e "Saved to: ${BOLD}${OUTPUT_FILE}${RESET}"
echo
echo -e "${YELLOW}Keep this key secure! It provides full access to RateWatch.${RESET}"
echo -e "Use it in API calls with: ${BOLD}Authorization: Bearer ${API_KEY}${RESET}"
echo

# Example usage
echo -e "${BOLD}Example Usage:${RESET}"
echo "curl -X POST http://localhost:8081/v1/check \\"
echo "  -H \"Authorization: Bearer ${API_KEY}\" \\"
echo "  -H \"Content-Type: application/json\" \\"
echo "  -d '{\"key\": \"user:123\", \"limit\": 100, \"window\": 3600, \"cost\": 1}'"
