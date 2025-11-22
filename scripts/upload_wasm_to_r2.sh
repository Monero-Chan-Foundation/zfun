#!/bin/bash
set -euo pipefail

# Upload WASM to Cloudflare R2
# This script uploads the built WASM file to R2 for frontend consumption

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Load environment variables
if [ -f "$REPO_ROOT/.env" ]; then
    source "$REPO_ROOT/.env"
fi

# Check required environment variables
if [ -z "${R2_ACCESS_KEY_ID:-}" ] || [ -z "${R2_SECRET_ACCESS_KEY:-}" ] || [ -z "${R2_ENDPOINT:-}" ] || [ -z "${R2_BUCKET:-}" ]; then
    echo -e "${RED}Error: R2 credentials not set in .env${NC}"
    echo "Please set R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY, R2_ENDPOINT, and R2_BUCKET"
    exit 1
fi

# Check if aws CLI is installed
if ! command -v aws &> /dev/null; then
    echo -e "${YELLOW}AWS CLI not found. Installing...${NC}"
    # Install aws CLI
    if command -v apt-get &> /dev/null; then
        sudo apt-get update && sudo apt-get install -y awscli
    elif command -v brew &> /dev/null; then
        brew install awscli
    else
        echo -e "${RED}Please install AWS CLI manually${NC}"
        exit 1
    fi
fi

# Get snapshot height from metadata
METADATA_FILE="$REPO_ROOT/snapshot_metadata.json"
if [ ! -f "$METADATA_FILE" ]; then
    echo -e "${RED}Error: snapshot_metadata.json not found${NC}"
    exit 1
fi

HEIGHT=$(jq -r '.snapshot_height // .height' "$METADATA_FILE")
if [ -z "$HEIGHT" ] || [ "$HEIGHT" = "null" ]; then
    echo -e "${RED}Error: Could not read snapshot height from metadata${NC}"
    exit 1
fi

# Find the WASM file
WASM_FILE="$REPO_ROOT/crates/wasm/pkg/zfun_wasm_bg.wasm"
if [ ! -f "$WASM_FILE" ]; then
    echo -e "${RED}Error: WASM file not found at $WASM_FILE${NC}"
    echo "Please build WASM first: cd crates/wasm && wasm-pack build --target web"
    exit 1
fi

echo -e "${GREEN}Uploading WASM to R2...${NC}"
echo "  Height: $HEIGHT"
echo "  File: $WASM_FILE"
echo "  Bucket: $R2_BUCKET"

# Configure AWS CLI for R2
export AWS_ACCESS_KEY_ID="$R2_ACCESS_KEY_ID"
export AWS_SECRET_ACCESS_KEY="$R2_SECRET_ACCESS_KEY"
export AWS_DEFAULT_REGION="auto"

# Upload versioned WASM (for archival and testing)
VERSIONED_KEY="wasm/zfun_wasm_bg-${HEIGHT}.wasm"
echo -e "${YELLOW}Uploading versioned WASM: $VERSIONED_KEY${NC}"
aws s3 cp "$WASM_FILE" "s3://${R2_BUCKET}/${VERSIONED_KEY}" \
    --endpoint-url "$R2_ENDPOINT" \
    --content-type "application/wasm" \
    --cache-control "public, max-age=31536000, immutable"

# Upload latest WASM (production use)
LATEST_KEY="wasm/latest/zfun_wasm_bg.wasm"
echo -e "${YELLOW}Uploading latest WASM: $LATEST_KEY${NC}"
aws s3 cp "$WASM_FILE" "s3://${R2_BUCKET}/${LATEST_KEY}" \
    --endpoint-url "$R2_ENDPOINT" \
    --content-type "application/wasm" \
    --cache-control "public, max-age=300"

# Upload JS glue code
JS_FILE="$REPO_ROOT/crates/wasm/pkg/zfun_wasm.js"
if [ -f "$JS_FILE" ]; then
    echo -e "${YELLOW}Uploading JS glue code...${NC}"

    # Versioned JS
    aws s3 cp "$JS_FILE" "s3://${R2_BUCKET}/wasm/zfun_wasm-${HEIGHT}.js" \
        --endpoint-url "$R2_ENDPOINT" \
        --content-type "application/javascript" \
        --cache-control "public, max-age=31536000, immutable"

    # Latest JS
    aws s3 cp "$JS_FILE" "s3://${R2_BUCKET}/wasm/latest/zfun_wasm.js" \
        --endpoint-url "$R2_ENDPOINT" \
        --content-type "application/javascript" \
        --cache-control "public, max-age=300"
fi

# Upload metadata.json for version tracking
echo -e "${YELLOW}Uploading metadata...${NC}"
aws s3 cp "$METADATA_FILE" "s3://${R2_BUCKET}/wasm/latest/metadata.json" \
    --endpoint-url "$R2_ENDPOINT" \
    --content-type "application/json" \
    --cache-control "public, max-age=300"

echo -e "${GREEN}✓ WASM upload complete!${NC}"
echo ""
echo "Versioned URLs (saved for testing):"
echo "  https://wasm.z.fun/wasm/zfun_wasm_bg-${HEIGHT}.wasm"
echo "  https://wasm.z.fun/wasm/zfun_wasm-${HEIGHT}.js"
echo ""
echo "Latest URLs (production):"
echo "  https://wasm.z.fun/wasm/latest/zfun_wasm_bg.wasm"
echo "  https://wasm.z.fun/wasm/latest/zfun_wasm.js"
echo "  https://wasm.z.fun/wasm/latest/metadata.json"
