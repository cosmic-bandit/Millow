#!/bin/bash
APP="/Applications/Millow.app"
ENTITLEMENTS="$(dirname $0)/src-tauri/Entitlements.plist"

echo "🔧 Removing old signature..."
codesign --remove-signature "$APP" 2>/dev/null

echo "🔧 Re-signing with entitlements..."
codesign --force --deep --sign - --entitlements "$ENTITLEMENTS" "$APP"

echo "🔧 Verifying..."
codesign -d --entitlements - "$APP" 2>&1 | head -5

echo "✅ Done! Restart Millow and re-add to Input Monitoring."
