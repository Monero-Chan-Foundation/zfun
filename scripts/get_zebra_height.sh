#!/bin/bash
set -euo pipefail

# Get current Zebra block height via RPC
# Usage: ./scripts/get_zebra_height.sh [--rpc-url URL]
#
# Returns: Block height as integer (e.g., 3140323)
# Exit codes:
#   0 - Success
#   1 - RPC failed or Zebra not accessible
#   2 - Zebra not fully synced

# Default RPC URL
RPC_URL="${ZEBRA_RPC_URL:-http://localhost:8232}"

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --rpc-url)
      RPC_URL="$2"
      shift 2
      ;;
    --help|-h)
      echo "Usage: $0 [OPTIONS]"
      echo ""
      echo "Get current Zebra block height via RPC"
      echo ""
      echo "Options:"
      echo "  --rpc-url URL    Zebra RPC endpoint (default: http://localhost:8232)"
      echo "  --help, -h       Show this help message"
      echo ""
      echo "Environment Variables:"
      echo "  ZEBRA_RPC_URL    Default RPC URL if --rpc-url not specified"
      echo ""
      echo "Returns:"
      echo "  Prints block height to stdout (e.g., 3140323)"
      echo ""
      echo "Exit Codes:"
      echo "  0 - Success"
      echo "  1 - RPC failed or Zebra not accessible"
      echo "  2 - Zebra not fully synced"
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Use --help for usage information" >&2
      exit 1
      ;;
  esac
done

# Query Zebra RPC for blockchain info
RESPONSE=$(curl -s -f "$RPC_URL" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"getblockchaininfo","params":[],"id":1}' \
  2>/dev/null) || {
    echo "Error: Failed to connect to Zebra RPC at $RPC_URL" >&2
    echo "Make sure Zebra is running and RPC is accessible" >&2
    exit 1
  }

# Check if response contains an error
if echo "$RESPONSE" | jq -e '.error' >/dev/null 2>&1; then
  ERROR_MSG=$(echo "$RESPONSE" | jq -r '.error.message')
  echo "Error: Zebra RPC returned error: $ERROR_MSG" >&2
  exit 1
fi

# Extract block height
HEIGHT=$(echo "$RESPONSE" | jq -r '.result.blocks')

if [ -z "$HEIGHT" ] || [ "$HEIGHT" = "null" ]; then
  echo "Error: Could not parse block height from RPC response" >&2
  echo "Response: $RESPONSE" >&2
  exit 1
fi

# Extract verification progress (0.0 to 1.0)
VERIFICATION_PROGRESS=$(echo "$RESPONSE" | jq -r '.result.verificationprogress')

# Check if Zebra is fully synced
# verificationprogress should be 1.0 (or very close) when fully synced
if [ "$VERIFICATION_PROGRESS" != "1" ] && [ "$VERIFICATION_PROGRESS" != "1.0" ]; then
  # Allow progress >= 0.9999 (effectively synced)
  if (( $(echo "$VERIFICATION_PROGRESS < 0.9999" | bc -l) )); then
    echo "Warning: Zebra not fully synced (progress: $VERIFICATION_PROGRESS)" >&2
    echo "Current height: $HEIGHT" >&2
    echo "Continuing anyway, but snapshot may be slightly behind network tip" >&2
  fi
fi

# Extract estimated height (network tip)
ESTIMATED_HEIGHT=$(echo "$RESPONSE" | jq -r '.result.estimatedheight')

# Warn if we're significantly behind the network
if [ -n "$ESTIMATED_HEIGHT" ] && [ "$ESTIMATED_HEIGHT" != "null" ]; then
  DIFF=$((ESTIMATED_HEIGHT - HEIGHT))
  if [ $DIFF -gt 100 ]; then
    echo "Warning: Local height ($HEIGHT) is $DIFF blocks behind network tip ($ESTIMATED_HEIGHT)" >&2
    echo "Zebra may still be syncing" >&2
  fi
fi

# Output just the height to stdout (for scripting)
echo "$HEIGHT"
