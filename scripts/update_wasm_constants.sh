#!/bin/bash
set -euo pipefail

# Update WASM snapshot metadata
# Usage: ./scripts/update_wasm_constants.sh <metadata.json>
#
# Copies snapshot metadata to snapshot_metadata.json which is included in WASM build

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUTPUT_FILE="$REPO_ROOT/snapshot_metadata.json"

# Parse arguments
if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <metadata.json>" >&2
  echo "" >&2
  echo "Updates snapshot_metadata.json from source snapshot metadata" >&2
  echo "" >&2
  echo "Example:" >&2
  echo "  $0 snapshots/snapshot-3140326/metadata.json" >&2
  exit 1
fi

METADATA_FILE="$1"

# Check if metadata file exists
if [[ ! -f "$METADATA_FILE" ]]; then
  echo "Error: Metadata file not found: $METADATA_FILE" >&2
  exit 1
fi

echo "Updating snapshot metadata..."
echo "  Source: $METADATA_FILE"
echo "  Target: $OUTPUT_FILE"
echo ""

# Validate it's valid JSON and has required fields
if ! jq -e '.orchard_count' "$METADATA_FILE" >/dev/null 2>&1; then
  echo "Error: Invalid metadata JSON or missing required field 'orchard_count'" >&2
  exit 1
fi

# Create backup if output file exists
if [[ -f "$OUTPUT_FILE" ]]; then
  BACKUP_FILE="$OUTPUT_FILE.backup.$(date +%s)"
  cp "$OUTPUT_FILE" "$BACKUP_FILE"
  echo "Created backup: $BACKUP_FILE"
fi

# Copy the metadata file as base
cp "$METADATA_FILE" "$OUTPUT_FILE"

# Merge in nullifier/utxo data from nullifiers/snapshot_metadata.json if it exists
SNAPSHOT_DIR=$(dirname "$METADATA_FILE")
NULLIFIER_METADATA="$SNAPSHOT_DIR/nullifiers/snapshot_metadata.json"

if [[ -f "$NULLIFIER_METADATA" ]]; then
  echo "Found nullifier metadata, merging..."

  # Extract nullifier/utxo fields from nullifier metadata
  NULLIFIER_ROOT=$(jq -r '.nullifier_root | if type == "array" then map(. | tostring) | join(",") else . end' "$NULLIFIER_METADATA")
  UTXO_ROOT=$(jq -r '.utxo_root | if type == "array" then map(. | tostring) | join(",") else . end' "$NULLIFIER_METADATA")
  NULLIFIER_COUNT=$(jq -r '.nullifier_count' "$NULLIFIER_METADATA")
  UTXO_COUNT=$(jq -r '.utxo_count' "$NULLIFIER_METADATA")

  # Convert byte arrays to hex if needed
  if [[ "$NULLIFIER_ROOT" =~ ^[0-9,]+$ ]]; then
    NULLIFIER_ROOT=$(echo "$NULLIFIER_ROOT" | python3 -c "import sys; print(''.join(format(int(x), '02x') for x in sys.stdin.read().strip().split(',')))")
  fi

  if [[ "$UTXO_ROOT" =~ ^[0-9,]+$ ]]; then
    UTXO_ROOT=$(echo "$UTXO_ROOT" | python3 -c "import sys; print(''.join(format(int(x), '02x') for x in sys.stdin.read().strip().split(',')))")
  fi

  # Merge into output file
  TMP_FILE="${OUTPUT_FILE}.tmp"
  jq --arg nr "$NULLIFIER_ROOT" \
     --arg ur "$UTXO_ROOT" \
     --argjson nc "$NULLIFIER_COUNT" \
     --argjson uc "$UTXO_COUNT" \
     '. + {nullifier_root: $nr, utxo_root: $ur, nullifier_count: $nc, utxo_count: $uc}' \
     "$OUTPUT_FILE" > "$TMP_FILE"
  mv "$TMP_FILE" "$OUTPUT_FILE"

  echo "  ✓ Merged nullifier_root: $NULLIFIER_ROOT"
  echo "  ✓ Merged utxo_root: $UTXO_ROOT"
  echo "  ✓ Merged nullifier_count: $NULLIFIER_COUNT"
  echo "  ✓ Merged utxo_count: $UTXO_COUNT"
else
  echo "Warning: No nullifier metadata found at $NULLIFIER_METADATA"
  echo "  nullifier_root and utxo_root will use build.rs defaults"
fi

# Display key values
HEIGHT=$(jq -r '.height // .snapshot_height' "$OUTPUT_FILE")
ORCHARD_COUNT=$(jq -r '.orchard_count' "$OUTPUT_FILE")
SAPLING_COUNT=$(jq -r '.sapling_count' "$OUTPUT_FILE")

echo ""
echo "✓ Successfully updated snapshot_metadata.json"
echo ""
echo "Snapshot info:"
echo "  Height:        $HEIGHT"
echo "  Orchard count: $ORCHARD_COUNT"
echo "  Sapling count: $SAPLING_COUNT"
echo ""
echo "Next steps:"
echo "  1. Rebuild WASM: cd frontend && bun run build:wasm"
echo "  2. Update server SNAPSHOT_DIR"
echo "  3. Restart server"
