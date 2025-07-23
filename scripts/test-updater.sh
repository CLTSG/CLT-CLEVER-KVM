#!/bin/bash

# Test script for Tauri auto-updater functionality
# This script helps you test the updater locally before deploying

set -e

echo "🧪 Testing Tauri Auto-Updater"
echo "============================="
echo ""

# Check if we're in the right directory
if [ ! -f "src-tauri/tauri.conf.json" ]; then
    echo "❌ Please run this script from the project root directory"
    exit 1
fi

# Check if keys exist
if [ ! -f "$HOME/.tauri/clever-kvm.key" ]; then
    echo "❌ Private key not found. Run this first:"
    echo "  npx tauri signer generate -w ~/.tauri/clever-kvm.key --password Enter_Password --force"
    exit 1
fi

echo "✅ Private key found"

# Check configuration
echo "🔍 Checking updater configuration..."

# Extract updater config from tauri.conf.json
UPDATER_ACTIVE=$(grep -A 10 '"updater"' src-tauri/tauri.conf.json | grep '"active"' | grep -o 'true\|false' || echo "false")

if [ "$UPDATER_ACTIVE" = "true" ]; then
    echo "✅ Updater is enabled in tauri.conf.json"
else
    echo "❌ Updater is not enabled in tauri.conf.json"
    echo "   Please set updater.active to true"
    exit 1
fi

# Check if public key is configured
if grep -q '"pubkey"' src-tauri/tauri.conf.json; then
    echo "✅ Public key is configured"
else
    echo "❌ Public key is not configured in tauri.conf.json"
    exit 1
fi

# Check if endpoint is configured
if grep -q '"endpoints"' src-tauri/tauri.conf.json; then
    echo "✅ Update endpoint is configured"
else
    echo "❌ Update endpoint is not configured"
    exit 1
fi

echo ""
echo "🏗️  Building application with updater..."

# Set environment variables for signing
export TAURI_PRIVATE_KEY="$HOME/.tauri/clever-kvm.key"
export TAURI_KEY_PASSWORD="Enter_Password"

# Build the application
npm install
npm run tauri:build

echo ""
echo "✅ Build completed successfully!"
echo ""
echo "📋 Next steps for testing:"
echo "=========================="
echo ""
echo "1. Create a test release on GitHub:"
echo "   - Create a new tag: git tag v1.1.0 && git push origin v1.1.0"
echo "   - This will trigger the GitHub Actions workflow"
echo ""
echo "2. Test the updater locally:"
echo "   - Run the built application"
echo "   - Check the 'Status' tab"
echo "   - Click 'Check for Updates'"
echo ""
echo "3. Simulate an update:"
echo "   - Build with version 1.1.0 first"
echo "   - Create a release with version 1.1.0"
echo "   - The app should detect the update"
echo ""
echo "🔧 Troubleshooting:"
echo "=================="
echo "- If updates aren't detected, check the GitHub release has all required files"
echo "- Ensure the endpoint URL in tauri.conf.json matches your repository"
echo "- Check browser dev tools for any network errors"
echo ""
echo "🎉 Auto-updater test preparation complete!"
