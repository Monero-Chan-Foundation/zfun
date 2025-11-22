#!/bin/bash
# Upload WASM files to Cloudflare R2
# Usage: ./upload-wasm.sh
#
# This script should be run from the frontend directory.
# It will look for WASM files in ../crates/wasm/pkg/

BUCKET_NAME="zfun-wasm"
WASM_PKG_DIR="../crates/wasm/pkg"

echo "🔐 Checking authentication..."
if ! wrangler whoami &>/dev/null; then
    echo "⚠️  Not authenticated. Please run: wrangler login"
    echo "   This will open a browser window for authentication."
    exit 1
fi

echo "✅ Authenticated!"
echo ""

# Check if pkg directory exists
if [ ! -d "$WASM_PKG_DIR" ]; then
    echo "❌ Error: $WASM_PKG_DIR directory not found!"
    echo "   Please build the WASM first:"
    echo "   cd ../crates/wasm && wasm-pack build --target web"
    exit 1
fi

# Check if required files exist
if [ ! -f "$WASM_PKG_DIR/zfun_wasm_bg.wasm" ] || [ ! -f "$WASM_PKG_DIR/zfun_wasm.js" ]; then
    echo "❌ Error: Required WASM files not found in $WASM_PKG_DIR"
    echo "   Please build the WASM first:"
    echo "   cd ../crates/wasm && wasm-pack build --target web"
    exit 1
fi

echo "📤 Uploading WASM files to R2 bucket: $BUCKET_NAME/wasm/latest/"
echo ""

# Upload WASM binary
echo "Uploading zfun_wasm_bg.wasm..."
wrangler r2 object put "$BUCKET_NAME/wasm/latest/zfun_wasm_bg.wasm" \
    --file="$WASM_PKG_DIR/zfun_wasm_bg.wasm" \
    --content-type="application/wasm"

if [ $? -ne 0 ]; then
    echo "❌ Failed to upload WASM binary!"
    exit 1
fi

# Upload JavaScript glue code
echo "Uploading zfun_wasm.js..."
wrangler r2 object put "$BUCKET_NAME/wasm/latest/zfun_wasm.js" \
    --file="$WASM_PKG_DIR/zfun_wasm.js" \
    --content-type="application/javascript"

if [ $? -ne 0 ]; then
    echo "❌ Failed to upload JavaScript glue code!"
    exit 1
fi

# Upload TypeScript definitions (optional but useful)
if [ -f "$WASM_PKG_DIR/zfun_wasm.d.ts" ]; then
    echo "Uploading zfun_wasm.d.ts..."
    wrangler r2 object put "$BUCKET_NAME/wasm/latest/zfun_wasm.d.ts" \
        --file="$WASM_PKG_DIR/zfun_wasm.d.ts" \
        --content-type="text/plain"
fi

echo ""
echo "✅ Upload complete!"
echo ""
echo "📋 Uploaded files:"
echo "   - $BUCKET_NAME/wasm/latest/zfun_wasm_bg.wasm"
echo "   - $BUCKET_NAME/wasm/latest/zfun_wasm.js"
echo "   - $BUCKET_NAME/wasm/latest/zfun_wasm.d.ts"
echo ""
echo "🔗 Files will be accessible at:"
echo "   https://wasm.z.fun/wasm/latest/zfun_wasm_bg.wasm"
echo "   https://wasm.z.fun/wasm/latest/zfun_wasm.js"
echo ""
echo "⚠️  IMPORTANT:"
echo "   1. Both files MUST be from the same wasm-pack build!"
echo "   2. Clear browser cache after uploading"
echo "   3. The frontend/pkg folder is NOT used - delete it to avoid confusion"
echo ""
echo "📝 To rebuild WASM:"
echo "   cd ../crates/wasm && wasm-pack build --target web"
