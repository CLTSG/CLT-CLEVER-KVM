#!/bin/bash

# macOS Universal Build Script
# This script attempts to build universal macOS binaries with proper FFmpeg configuration

set -e

echo "Setting up environment for universal macOS build..."

# Set macOS deployment target
export MACOSX_DEPLOYMENT_TARGET=10.13

# Allow cross-compilation for pkg-config
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
export PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1

# Set up FFmpeg paths for both architectures
if [[ -d "/opt/homebrew" ]]; then
    # Apple Silicon Mac
    export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    export LIBRARY_PATH="/opt/homebrew/lib:${LIBRARY_PATH:-}"
    export CPATH="/opt/homebrew/include:${CPATH:-}"
fi

if [[ -d "/usr/local" ]]; then
    # Intel Mac
    export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
    export LIBRARY_PATH="/usr/local/lib:${LIBRARY_PATH:-}"
    export CPATH="/usr/local/include:${CPATH:-}"
fi

echo "PKG_CONFIG_PATH: $PKG_CONFIG_PATH"
echo "LIBRARY_PATH: $LIBRARY_PATH"
echo "CPATH: $CPATH"

# Verify FFmpeg installation
echo "Checking FFmpeg installation..."
pkg-config --exists libavutil && echo "✓ libavutil found" || echo "✗ libavutil not found"
pkg-config --exists libavcodec && echo "✓ libavcodec found" || echo "✗ libavcodec not found"
pkg-config --exists libavformat && echo "✓ libavformat found" || echo "✗ libavformat not found"

# Install npm dependencies
echo "Installing npm dependencies..."
npm ci

# Attempt universal build
echo "Building universal macOS binary..."
if npm run tauri build -- --target universal-apple-darwin; then
    echo "✓ Universal build successful!"
else
    echo "✗ Universal build failed, attempting fallback..."
    
    # Fallback: build separate architectures
    echo "Building x86_64 binary..."
    npm run tauri build -- --target x86_64-apple-darwin
    
    echo "Building aarch64 binary..."
    npm run tauri build -- --target aarch64-apple-darwin
    
    echo "Note: Built separate binaries instead of universal binary"
fi

echo "Build process completed!"
