#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}Uploading active shards to R2...${NC}"

# Load environment variables from .env
if [ -f "$REPO_ROOT/.env" ]; then
    source "$REPO_ROOT/.env"
fi

# Check required environment variables
if [ -z "${R2_ACCESS_KEY_ID:-}" ] || [ -z "${R2_SECRET_ACCESS_KEY:-}" ] || [ -z "${R2_ENDPOINT:-}" ] || [ -z "${R2_BUCKET:-}" ]; then
    echo -e "${RED}Error: R2 credentials not set in .env${NC}"
    echo "Please set R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY, R2_ENDPOINT, and R2_BUCKET"
    exit 1
fi

# Configure AWS CLI for R2
export AWS_ACCESS_KEY_ID="$R2_ACCESS_KEY_ID"
export AWS_SECRET_ACCESS_KEY="$R2_SECRET_ACCESS_KEY"
export AWS_DEFAULT_REGION="auto"

# Get snapshot height from root metadata
METADATA_FILE="$REPO_ROOT/snapshot_metadata.json"
if [ ! -f "$METADATA_FILE" ]; then
    echo -e "${RED}✗ Metadata file not found: $METADATA_FILE${NC}"
    exit 1
fi

# Extract snapshot height and counts
SNAPSHOT_HEIGHT=$(jq -r '.snapshot_height' "$METADATA_FILE")
ORCHARD_COUNT=$(jq -r '.orchard_count' "$METADATA_FILE")
SAPLING_COUNT=$(jq -r '.sapling_count' "$METADATA_FILE")

# Calculate active shard IDs
ORCHARD_SHARD_ID=$((ORCHARD_COUNT / 65536))
SAPLING_SHARD_ID=$((SAPLING_COUNT / 65536))

SNAPSHOT_DIR="$REPO_ROOT/snapshots/snapshot-${SNAPSHOT_HEIGHT}"

echo "  Height: $SNAPSHOT_HEIGHT"
echo "  Orchard active shard: $ORCHARD_SHARD_ID"
echo "  Sapling active shard: $SAPLING_SHARD_ID"
echo "  Bucket: $R2_BUCKET"
echo ""

# Check if AWS CLI is available
if ! command -v aws &> /dev/null; then
    echo -e "${RED}✗ AWS CLI not found. Please install it first.${NC}"
    exit 1
fi

upload_shard() {
    local pool=$1
    local shard_id=$2
    local shard_path="$SNAPSHOT_DIR/shards/$pool/${shard_id}.bin"

    if [ ! -f "$shard_path" ]; then
        echo -e "${YELLOW}⚠ Shard not found: $shard_path${NC}"
        return 1
    fi

    local s3_path="s3://${R2_BUCKET}/snapshots/${SNAPSHOT_HEIGHT}/${pool}/${shard_id}.bin"

    echo -e "${YELLOW}Uploading $pool shard $shard_id...${NC}"

    if aws s3 cp "$shard_path" "$s3_path" \
        --endpoint-url "$R2_ENDPOINT" \
        --content-type "application/octet-stream" \
        --cache-control "public, max-age=31536000, immutable"; then
        echo -e "${GREEN}✓ Uploaded $pool shard $shard_id${NC}"
        return 0
    else
        echo -e "${RED}✗ Failed to upload $pool shard $shard_id${NC}"
        return 1
    fi
}

# Upload orchard active shard
echo ""
if ! upload_shard "orchard" "$ORCHARD_SHARD_ID"; then
    echo -e "${RED}✗ Failed to upload Orchard shard${NC}"
    exit 1
fi

# Upload sapling active shard
echo ""
if ! upload_shard "sapling" "$SAPLING_SHARD_ID"; then
    echo -e "${RED}✗ Failed to upload Sapling shard${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}✓ Shard upload complete!${NC}"
echo ""
echo "Active shards are now available at:"
echo "  https://wasm.z.fun/snapshots/${SNAPSHOT_HEIGHT}/orchard/${ORCHARD_SHARD_ID}.bin"
echo "  https://wasm.z.fun/snapshots/${SNAPSHOT_HEIGHT}/sapling/${SAPLING_SHARD_ID}.bin"
echo ""
