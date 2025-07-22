#!/bin/bash

# FFmpeg Build Troubleshooting Script
# This script helps diagnose and fix FFmpeg-related build issues

set -e

echo "🔍 FFmpeg Build Troubleshooting Script"
echo "======================================"

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to check pkg-config
check_pkg_config() {
    echo "📦 Checking pkg-config..."
    if command_exists pkg-config; then
        echo "✅ pkg-config is installed: $(pkg-config --version)"
        
        # Check for libavutil specifically
        if pkg-config --exists libavutil; then
            echo "✅ libavutil found: $(pkg-config --modversion libavutil)"
            echo "   Cflags: $(pkg-config --cflags libavutil)"
            echo "   Libs: $(pkg-config --libs libavutil)"
        else
            echo "❌ libavutil not found via pkg-config"
            echo "   PKG_CONFIG_PATH: ${PKG_CONFIG_PATH:-not set}"
            return 1
        fi
    else
        echo "❌ pkg-config not found"
        return 1
    fi
}

# Function to check FFmpeg
check_ffmpeg() {
    echo "🎥 Checking FFmpeg..."
    if command_exists ffmpeg; then
        echo "✅ FFmpeg is installed"
        ffmpeg -version | head -1
    else
        echo "❌ FFmpeg not found"
        return 1
    fi
}

# Function to install dependencies based on OS
install_dependencies() {
    echo "📥 Installing dependencies..."
    
    case "$(uname -s)" in
        Linux*)
            echo "🐧 Detected Linux"
            if command_exists apt-get; then
                echo "Installing via apt-get..."
                sudo apt-get update
                sudo apt-get install -y ffmpeg libavcodec-dev libavformat-dev libavutil-dev \
                    libavdevice-dev libavfilter-dev libswscale-dev libswresample-dev pkg-config
            elif command_exists yum; then
                echo "Installing via yum..."
                sudo yum install -y ffmpeg-devel pkgconfig
            elif command_exists pacman; then
                echo "Installing via pacman..."
                sudo pacman -S ffmpeg pkg-config
            else
                echo "❌ No supported package manager found"
                return 1
            fi
            ;;
        Darwin*)
            echo "🍎 Detected macOS"
            if command_exists brew; then
                echo "Installing via Homebrew..."
                brew install ffmpeg pkg-config
            else
                echo "❌ Homebrew not found. Please install Homebrew first."
                return 1
            fi
            ;;
        CYGWIN*|MINGW*|MSYS*)
            echo "🪟 Detected Windows"
            if command_exists choco; then
                echo "Installing via Chocolatey..."
                choco install ffmpeg pkgconfiglite -y
            else
                echo "❌ Chocolatey not found. Please install Chocolatey first."
                echo "   Or manually install FFmpeg and pkg-config"
                return 1
            fi
            ;;
        *)
            echo "❌ Unsupported operating system: $(uname -s)"
            return 1
            ;;
    esac
}

# Function to set environment variables
set_env_vars() {
    echo "🔧 Setting environment variables..."
    export PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
    export PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1
    echo "✅ Set PKG_CONFIG_ALLOW_SYSTEM_LIBS=1"
    echo "✅ Set PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1"
}

# Function to test Rust build
test_build() {
    echo "🦀 Testing Rust build..."
    cd src-tauri
    
    # Check if ffmpeg-next can find dependencies
    if cargo check 2>&1 | grep -q "ffmpeg-sys-next"; then
        echo "❌ FFmpeg build issue detected"
        echo "🔧 Trying to resolve..."
        
        set_env_vars
        
        echo "🔄 Retrying cargo check..."
        if cargo check; then
            echo "✅ Build check passed!"
        else
            echo "❌ Build check failed"
            return 1
        fi
    else
        echo "✅ No FFmpeg build issues detected"
    fi
    
    cd ..
}

# Main execution
main() {
    echo "Starting troubleshooting process..."
    echo ""
    
    # Check current status
    check_pkg_config || echo "⚠️ pkg-config issues detected"
    echo ""
    
    check_ffmpeg || echo "⚠️ FFmpeg issues detected"
    echo ""
    
    # Ask if user wants to install dependencies
    echo "❓ Would you like to install/reinstall dependencies? (y/n)"
    read -r response
    if [[ "$response" =~ ^[Yy]$ ]]; then
        install_dependencies
        echo ""
        
        # Re-check after installation
        echo "🔄 Re-checking after installation..."
        check_pkg_config
        echo ""
        check_ffmpeg
        echo ""
    fi
    
    # Set environment variables
    set_env_vars
    echo ""
    
    # Test build
    echo "❓ Would you like to test the Rust build? (y/n)"
    read -r response
    if [[ "$response" =~ ^[Yy]$ ]]; then
        test_build
    fi
    
    echo ""
    echo "🎉 Troubleshooting complete!"
    echo ""
    echo "💡 If you're still having issues:"
    echo "   1. Make sure PKG_CONFIG_PATH is set correctly"
    echo "   2. Try: export PKG_CONFIG_ALLOW_SYSTEM_LIBS=1"
    echo "   3. Try: export PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1"
    echo "   4. Check that libavutil.pc exists in your pkg-config path"
    echo "   5. Run: pkg-config --debug libavutil 2>&1 | head -20"
}

# Run main function
main "$@"
