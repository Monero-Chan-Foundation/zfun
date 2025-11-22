#!/bin/bash
set -euo pipefail

# Z.FUN Snapshot Builder
# Builds complete snapshot including:
# - Orchard/Sapling shards
# - UTXO tree
# - Nullifier tree
# - Metadata

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Change to project root (one level up from scripts/)
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Hardcoded default values
ZEBRA_DIR="/home/ubuntu/zebra/zebra"
NETWORK="mainnet"
BUILD_NULLIFIERS=true
BUILD_UTXOS=true
VERIFY=false
SKIP_SHARDS=false
ONLY_NULLIFIERS=false

# Parse command-line arguments
HEIGHT=""
while [[ $# -gt 0 ]]; do
  case $1 in
    --height)
      if [[ $# -gt 1 ]]; then
        HEIGHT="$2"
        shift 2
      else
        echo "Error: --height requires a value"
        exit 1
      fi
      ;;
    --skip-shards)
      SKIP_SHARDS=true
      shift
      ;;
    --only-nullifiers)
      ONLY_NULLIFIERS=true
      SKIP_SHARDS=true
      BUILD_UTXOS=false
      VERIFY=false
      shift
      ;;
    *)
      echo "Unknown option: $1"
      echo "Usage: $0 [--height <HEIGHT>] [--skip-shards] [--only-nullifiers]"
      exit 1
      ;;
  esac
done

# Get height from Zebra RPC if not provided
# Note: RPC getblockcount returns the network tip, which may be ahead of what's
# actually synced in the database. If you get a "Block not found" error, the
# error message will show the highest available height - use that with --height
if [ -z "$HEIGHT" ]; then
  echo "Fetching current Zebra block height..."
  RPC_HEIGHT=$(curl -s http://localhost:8232 \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"getblockcount","params":[],"id":1}' \
    | jq -r '.result')

  if [ -z "$RPC_HEIGHT" ] || [ "$RPC_HEIGHT" = "null" ]; then
    echo "Error: Failed to get block height from Zebra RPC"
    echo "Make sure Zebra is running and RPC is accessible at http://localhost:8232"
    echo "Or provide height manually: $0 --height <HEIGHT>"
    exit 1
  fi
  
  HEIGHT="$RPC_HEIGHT"
  echo "RPC reports block height: $HEIGHT"
  echo "Note: Database may be synced to a lower height. If build fails, check error message for actual synced height."
fi

# Set output directory based on height
OUTPUT_DIR="snapshots/snapshot-$HEIGHT"

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo "=========================================="
echo "Z.FUN Snapshot Builder"
echo "=========================================="
echo "Height:       $HEIGHT"
echo "Network:      $NETWORK"
echo "Zebra dir:    $ZEBRA_DIR"
echo "Output dir:   $OUTPUT_DIR"
echo "Skip shards:      $SKIP_SHARDS"
echo "Build nullifiers: $BUILD_NULLIFIERS"
echo "Build UTXOs:      $BUILD_UTXOS"
echo "Verify:           $VERIFY"
echo "=========================================="
echo ""

START_TIME=$(date +%s)

# Phase 1: Build shards from Zebra
if [ "$SKIP_SHARDS" = false ]; then
  echo "=== Phase 1: Building Orchard/Sapling Shards ==="
  echo ""

  cargo run --release --bin snapshot_build --features zebra -- \
    --height "$HEIGHT" \
    --output-dir "$OUTPUT_DIR" \
    --zebra-cache-dir "$ZEBRA_DIR" \
    --network "$NETWORK"

  PHASE1_TIME=$(date +%s)
  echo ""
  echo "Phase 1 completed in $((PHASE1_TIME - START_TIME))s"
  echo ""
else
  echo "=== Phase 1: Skipped (--skip-shards flag) ==="
  echo ""
  PHASE1_TIME=$START_TIME
fi

# Phase 2: Build nullifier tree (if enabled)
if [ "$BUILD_NULLIFIERS" = true ]; then
  echo "=== Phase 2: Building Nullifier Tree ==="
  echo ""

  # Use zfun-setup-script to build nullifiers
  # Note: zfun-setup-script uses positional arguments:
  #   <zebra-cache-dir> <target-data-dir>
  # The target-data-dir must be empty or non-existent
  # We'll use a subdirectory for the nullifier data
  NULLIFIER_DIR="$OUTPUT_DIR/nullifiers"
  
  # Remove the nullifier dir if it exists (to ensure it's empty)
  if [ -d "$NULLIFIER_DIR" ]; then
    rm -rf "$NULLIFIER_DIR"
  fi
  
  cargo run --release --bin zfun-setup-script -- \
    "$ZEBRA_DIR" \
    "$NULLIFIER_DIR" \
    "$HEIGHT"

  PHASE2_TIME=$(date +%s)
  echo ""
  echo "Phase 2 completed in $((PHASE2_TIME - PHASE1_TIME))s"
  echo ""
else
  echo "=== Phase 2: Skipped (nullifiers disabled) ==="
  echo ""
  PHASE2_TIME=$PHASE1_TIME
fi

# Phase 3: Build UTXO tree (if enabled)
if [ "$BUILD_UTXOS" = true ]; then
  echo "=== Phase 3: Building UTXO Tree ==="
  echo ""

  # UTXO tree building would go here
  # For now, setup-script handles this

  PHASE3_TIME=$(date +%s)
  echo ""
  echo "Phase 3 completed in $((PHASE3_TIME - PHASE2_TIME))s"
  echo ""
else
  echo "=== Phase 3: Skipped (UTXOs disabled) ==="
  echo ""
  PHASE3_TIME=$PHASE2_TIME
fi

# Phase 4: Verify (if enabled)
if [ "$VERIFY" = true ]; then
  echo "=== Phase 4: Verifying Snapshot ==="
  echo ""

  cargo run --release --bin snapshot_verify -- \
    --snapshot-dir "$OUTPUT_DIR"

  PHASE4_TIME=$(date +%s)
  echo ""
  echo "Phase 4 completed in $((PHASE4_TIME - PHASE3_TIME))s"
  echo ""
else
  echo "=== Phase 4: Skipped (verification disabled) ==="
  echo ""
  PHASE4_TIME=$PHASE3_TIME
fi

END_TIME=$(date +%s)
TOTAL_TIME=$((END_TIME - START_TIME))

# Final summary
echo "=========================================="
echo "Snapshot Build Complete!"
echo "=========================================="
echo "Total time: ${TOTAL_TIME}s"
echo ""
echo "Output directory: $OUTPUT_DIR"
echo ""
echo "Contents:"
ls -lh "$OUTPUT_DIR"
echo ""

if [ -f "$OUTPUT_DIR/metadata.json" ]; then
  echo "Metadata summary:"
  jq '{height, orchard_count, sapling_count, orchard_root: .orchard_root[0:16], sapling_root: .sapling_root[0:16]}' "$OUTPUT_DIR/metadata.json"
fi

echo ""
echo "To use this snapshot, set:"
echo "  export SNAPSHOT_DIR=$OUTPUT_DIR"
