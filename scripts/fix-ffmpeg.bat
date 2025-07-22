@echo off
REM FFmpeg Build Troubleshooting Script for Windows
REM This script helps diagnose and fix FFmpeg-related build issues on Windows

echo.
echo 🔍 FFmpeg Build Troubleshooting Script (Windows)
echo ===============================================
echo.

REM Check if vcpkg is available
if exist "C:\vcpkg\vcpkg.exe" (
    echo ✅ vcpkg is available at C:\vcpkg
    goto :install_ffmpeg
)

echo ❌ vcpkg not found
echo 📥 Installing vcpkg...
git clone https://github.com/Microsoft/vcpkg.git C:\vcpkg
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Failed to clone vcpkg
    goto :end
)

cd C:\vcpkg
.\bootstrap-vcpkg.bat
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Failed to bootstrap vcpkg
    goto :end
)

.\vcpkg.exe integrate install
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Failed to integrate vcpkg
    goto :end
)

echo ✅ vcpkg installed and integrated

:install_ffmpeg
echo 📥 Installing FFmpeg via vcpkg...
cd C:\vcpkg
.\vcpkg.exe install ffmpeg[avcodec,avformat,avdevice,avfilter,swresample,swscale]:x64-windows
if %ERRORLEVEL% NEQ 0 (
    echo ❌ Failed to install FFmpeg
    goto :end
) else (
    echo ✅ FFmpeg installed via vcpkg
)

echo.
echo 🔧 Setting environment variables...
set VCPKG_ROOT=C:\vcpkg
set PKG_CONFIG_PATH=C:\vcpkg\installed\x64-windows\lib\pkgconfig
set PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
set PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1
echo ✅ Set VCPKG_ROOT=C:\vcpkg
echo ✅ Set PKG_CONFIG_PATH=C:\vcpkg\installed\x64-windows\lib\pkgconfig
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
echo    2. Make sure these environment variables are set:
echo       - VCPKG_ROOT=C:\vcpkg
echo       - PKG_CONFIG_PATH=C:\vcpkg\installed\x64-windows\lib\pkgconfig
echo       - PKG_CONFIG_ALLOW_SYSTEM_LIBS=1
echo       - PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1
echo    3. Verify FFmpeg pkg-config files exist:
echo       dir "C:\vcpkg\installed\x64-windows\lib\pkgconfig\libav*.pc"
echo    4. Try building with: npm run tauri:build
echo.

:end
pause
