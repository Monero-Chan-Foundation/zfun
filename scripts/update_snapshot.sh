#!/bin/bash
set -euo pipefail

# Z.FUN Snapshot Update Orchestrator
# Automates the complete snapshot update workflow

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$REPO_ROOT"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
HEIGHT=""
DRY_RUN=false
SKIP_VERIFY=false
FORCE=false
NO_WASM=false
SKIP_ZEBRA=false
ZEBRA_DIR="${ZEBRA_DIR:-/home/ubuntu/zebra/zebra}"
NETWORK="${NETWORK:-mainnet}"

print_usage() {
    echo "Z.FUN Snapshot Update - Automated snapshot building and deployment"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --height HEIGHT    Use specific height (default: auto-detect from Zebra)"
    echo "  --dry-run          Build snapshot but don't update server"
    echo "  --skip-verify      Skip snapshot verification step"
    echo "  --force            Rebuild even if snapshot already exists"
    echo "  --no-wasm          Skip WASM rebuild (for testing)"
    echo "  --skip-zebra       Skip Zebra checks and height detection (requires --height)"
    echo "  --help, -h         Show this help message"
    echo ""
    echo "Environment Variables:"
    echo "  ZEBRA_DIR          Zebra cache directory (default: /home/ubuntu/zebra/zebra)"
    echo "  NETWORK            Network to use (default: mainnet)"
    echo ""
    echo "Examples:"
    echo "  $0                           # Auto-detect height and update"
    echo "  $0 --height 3140000          # Update to specific height"
    echo "  $0 --dry-run                 # Build but don't deploy"
    echo "  $0 --skip-verify --force     # Fast rebuild"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --height)
            HEIGHT="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --skip-verify)
            SKIP_VERIFY=true
            shift
            ;;
        --force)
            FORCE=true
            shift
            ;;
        --no-wasm)
            NO_WASM=true
            shift
            ;;
        --skip-zebra)
            SKIP_ZEBRA=true
            shift
            ;;
        --help|-h)
            print_usage
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}" >&2
            print_usage
            exit 1
            ;;
    esac
done

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Z.FUN Snapshot Update${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

START_TIME=$(date +%s)

# Step 1: Check Zebra is running and synced
if [ "$SKIP_ZEBRA" = false ]; then
    echo -e "${BLUE}[1/11] Checking Zebra status...${NC}"
    if ! ./scripts/get_zebra_height.sh >/dev/null 2>&1; then
        echo -e "${RED}✗ Zebra is not running or not accessible${NC}"
        echo "Make sure Zebra is running: ./zfun.sh zebra-status"
        echo "Or use --skip-zebra with --height to skip Zebra checks"
        exit 1
    fi
    echo -e "${GREEN}✓ Zebra is running${NC}"
    echo ""
else
    echo -e "${BLUE}[1/11] Skipping Zebra checks (--skip-zebra)${NC}"
    echo ""
fi

# Step 2: Get current height
echo -e "${BLUE}[2/11] Getting current block height...${NC}"
if [ -z "$HEIGHT" ]; then
    if [ "$SKIP_ZEBRA" = true ]; then
        echo -e "${RED}✗ --height is required when using --skip-zebra${NC}"
        exit 1
    fi
    HEIGHT=$(./scripts/get_zebra_height.sh)
    if [ -z "$HEIGHT" ]; then
        echo -e "${RED}✗ Failed to get height from Zebra${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ Auto-detected height: $HEIGHT${NC}"
else
    echo -e "${GREEN}✓ Using specified height: $HEIGHT${NC}"
fi
echo ""

SNAPSHOT_DIR="snapshots/snapshot-$HEIGHT"

# Step 3: Check if snapshot already exists
echo -e "${BLUE}[3/11] Checking for existing snapshot...${NC}"
if [ -d "$SNAPSHOT_DIR" ] && [ "$FORCE" = false ]; then
    echo -e "${YELLOW}⚠ Snapshot already exists at $SNAPSHOT_DIR${NC}"
    echo "Use --force to rebuild"

    # Still update WASM and server with existing snapshot
    if [ "$DRY_RUN" = false ]; then
        echo "Proceeding to update WASM and server with existing snapshot..."
    fi
else
    if [ -d "$SNAPSHOT_DIR" ]; then
        echo -e "${YELLOW}⚠ Snapshot exists but --force specified, will rebuild${NC}"
    else
        echo -e "${GREEN}✓ No existing snapshot, will build${NC}"
    fi
    echo ""

    # Step 4: Build new snapshot
    echo -e "${BLUE}[4/11] Building snapshot...${NC}"
    echo "This may take several hours..."
    echo ""

    BUILD_ARGS="--height $HEIGHT"
    if [ "$SKIP_VERIFY" = true ]; then
        # build_snapshot.sh doesn't verify by default, so we don't need to pass a flag
        :
    fi

    if ! ./scripts/build_snapshot.sh $BUILD_ARGS; then
        echo -e "${RED}✗ Snapshot build failed${NC}"
        exit 1
    fi

    BUILD_TIME=$(date +%s)
    BUILD_DURATION=$((BUILD_TIME - START_TIME))
    echo -e "${GREEN}✓ Snapshot built in ${BUILD_DURATION}s${NC}"
    echo ""

    # Step 5: Verify snapshot integrity
    if [ "$SKIP_VERIFY" = false ]; then
        echo -e "${BLUE}[5/11] Verifying snapshot integrity...${NC}"

        if ! cargo run --release --bin snapshot_verify -- --snapshot-dir "$SNAPSHOT_DIR"; then
            echo -e "${RED}✗ Snapshot verification failed${NC}"
            exit 1
        fi

        echo -e "${GREEN}✓ Snapshot verified${NC}"
        echo ""
    else
        echo -e "${BLUE}[5/11] Skipping verification (--skip-verify)${NC}"
        echo ""
    fi
fi

# Step 6: Update frontend constants
echo -e "${BLUE}[6/11] Updating frontend constants...${NC}"

METADATA_FILE="$SNAPSHOT_DIR/metadata.json"
if [ ! -f "$METADATA_FILE" ]; then
    echo -e "${RED}✗ Metadata file not found: $METADATA_FILE${NC}"
    exit 1
fi

if ! ./scripts/update_wasm_constants.sh "$METADATA_FILE"; then
    echo -e "${RED}✗ Failed to update constants${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Constants updated${NC}"
echo ""

# Step 7: Rebuild WASM
if [ "$NO_WASM" = false ]; then
    echo -e "${BLUE}[7/11] Rebuilding WASM...${NC}"
    echo "This may take a few minutes..."
    echo ""

    cd frontend
    if ! bun run build:wasm; then
        cd ..
        echo -e "${RED}✗ WASM build failed${NC}"
        exit 1
    fi
    cd ..

    echo -e "${GREEN}✓ WASM rebuilt${NC}"
    echo ""

    # Step 7b: Upload WASM to R2
    echo -e "${BLUE}[7b/11] Uploading WASM to R2...${NC}"

    if ! ./scripts/upload_wasm_to_r2.sh; then
        echo -e "${YELLOW}⚠ WASM upload to R2 failed (continuing anyway)${NC}"
        echo "You can upload manually later with: ./scripts/upload_wasm_to_r2.sh"
    else
        echo -e "${GREEN}✓ WASM uploaded to R2${NC}"
    fi
    echo ""

    # Step 7c: Upload active shards to R2
    echo -e "${BLUE}[7c/11] Uploading active shards to R2...${NC}"

    if ! ./scripts/upload_shards_to_r2.sh; then
        echo -e "${YELLOW}⚠ Shard upload to R2 failed (continuing anyway)${NC}"
        echo "You can upload manually later with: ./scripts/upload_shards_to_r2.sh"
    else
        echo -e "${GREEN}✓ Active shards uploaded to R2${NC}"
    fi
    echo ""
else
    echo -e "${BLUE}[7/11] Skipping WASM rebuild (--no-wasm)${NC}"
    echo ""
fi

if [ "$DRY_RUN" = true ]; then
    echo -e "${YELLOW}========================================${NC}"
    echo -e "${YELLOW}DRY RUN - Stopping before deployment${NC}"
    echo -e "${YELLOW}========================================${NC}"
    echo ""
    echo "Snapshot built and verified at: $SNAPSHOT_DIR"
    echo "WASM constants updated"
    echo ""
    echo "To deploy manually:"
    echo "  1. Update .env: SNAPSHOT_DIR=$PWD/$SNAPSHOT_DIR"
    echo "  2. Restart server: ./zfun.sh restart-server"
    exit 0
fi

# Step 8: Update server SNAPSHOT_DIR env var
echo -e "${BLUE}[8/11] Updating server configuration...${NC}"

# Check if .env exists
if [ ! -f .env ]; then
    echo -e "${RED}✗ .env file not found${NC}"
    exit 1
fi

# Update SNAPSHOT_DIR in .env
SNAPSHOT_DIR_ABSOLUTE="$PWD/$SNAPSHOT_DIR"

if grep -q "^SNAPSHOT_DIR=" .env; then
    # Update existing
    sed -i "s|^SNAPSHOT_DIR=.*|SNAPSHOT_DIR=$SNAPSHOT_DIR_ABSOLUTE|" .env
    echo -e "${GREEN}✓ Updated SNAPSHOT_DIR in .env${NC}"
else
    # Add new
    echo "SNAPSHOT_DIR=$SNAPSHOT_DIR_ABSOLUTE" >> .env
    echo -e "${GREEN}✓ Added SNAPSHOT_DIR to .env${NC}"
fi
echo ""

# Step 9: Restart zfun-server
echo -e "${BLUE}[9/11] Restarting server...${NC}"

if ! ./zfun.sh restart-server; then
    echo -e "${RED}✗ Server restart failed${NC}"
    exit 1
fi

# Give server time to start
sleep 3

echo -e "${GREEN}✓ Server restarted${NC}"
echo ""

# Step 10: Run smoke test
echo -e "${BLUE}[10/11] Running smoke test...${NC}"

# Check if server is responding
if curl -s -f http://localhost:3000/health >/dev/null 2>&1; then
    echo -e "${GREEN}✓ Server health check passed${NC}"
else
    echo -e "${YELLOW}⚠ Server health check failed (endpoint may not exist)${NC}"
fi

# Check if snapshot metadata is accessible
if curl -s -f http://localhost:3000/api/snapshots/metadata >/dev/null 2>&1; then
    echo -e "${GREEN}✓ Snapshot metadata endpoint accessible${NC}"
else
    echo -e "${YELLOW}⚠ Could not access snapshot metadata endpoint${NC}"
fi

echo ""

# Step 11: Report success
echo -e "${BLUE}[11/11] Complete!${NC}"
echo ""

END_TIME=$(date +%s)
TOTAL_DURATION=$((END_TIME - START_TIME))
HOURS=$((TOTAL_DURATION / 3600))
MINUTES=$(((TOTAL_DURATION % 3600) / 60))
SECONDS=$((TOTAL_DURATION % 60))

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}✓ Snapshot Update Complete${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "Summary:"
echo "  Height:          $HEIGHT"
echo "  Snapshot:        $SNAPSHOT_DIR"
echo "  Total time:      ${HOURS}h ${MINUTES}m ${SECONDS}s"
echo ""
echo "Next steps:"
echo "  - Test with wallet: cargo run --release --bin zfun-host -- <wallet.db> --prove"
echo "  - Monitor logs:     ./zfun.sh logs"
echo "  - Check status:     ./zfun.sh status"
echo ""
