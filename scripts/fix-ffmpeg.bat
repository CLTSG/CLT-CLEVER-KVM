@echo off
REM FFmpeg Build Troubleshooting Script for Windows
REM This script helps diagnose and fix FFmpeg-related build issues on Windows

echo.
echo 🔍 FFmpeg Build Troubleshooting Script (Windows)
echo ===============================================
echo.

REM Check if chocolatey is available
where choco >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Chocolatey not found
    echo    Please install Chocolatey from https://chocolatey.org/install
    echo    Or manually install FFmpeg and pkg-config
    goto :end
) else (
    echo ✅ Chocolatey is available
)

REM Check if ffmpeg is available
where ffmpeg >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo ❌ FFmpeg not found
    echo 📥 Installing FFmpeg via Chocolatey...
    choco install ffmpeg -y
    if %ERRORLEVEL% NEQ 0 (
        echo ❌ Failed to install FFmpeg
        goto :end
    )
    echo ✅ FFmpeg installed
) else (
    echo ✅ FFmpeg is available
    ffmpeg -version 2>&1 | findstr "ffmpeg version"
)

REM Check if pkg-config is available
where pkg-config >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo ❌ pkg-config not found
    echo 📥 Installing pkg-config via Chocolatey...
    choco install pkgconfiglite -y
    if %ERRORLEVEL% NEQ 0 (
        echo ❌ Failed to install pkg-config
        goto :end
    )
    echo ✅ pkg-config installed
) else (
    echo ✅ pkg-config is available
    pkg-config --version
)

REM Set environment variables
echo.
echo 🔧 Setting environment variables...
set PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
set PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1
echo ✅ Set PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
echo ✅ Set PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1

REM Test build
echo.
echo ❓ Would you like to test the Rust build? (y/n)
set /p "response="
if /I "%response%"=="y" (
    echo 🦀 Testing Rust build...
    cd src-tauri
    cargo check
    if %ERRORLEVEL% EQU 0 (
        echo ✅ Build check passed!
    ) else (
        echo ❌ Build check failed
        echo    Try running this script again or check the error messages above
    )
    cd ..
)

echo.
echo 🎉 Troubleshooting complete!
echo.
echo 💡 If you're still having issues:
echo    1. Restart your terminal/command prompt
echo    2. Make sure FFmpeg and pkg-config are in your PATH
echo    3. Set these environment variables permanently:
echo       - PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
echo       - PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1
echo    4. Try building with: npm run tauri:build
echo.

:end
pause
