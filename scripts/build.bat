@echo off
REM Build script for Clever KVM (Windows)
REM This script builds the application for Windows

echo 🚀 Building Clever KVM...

REM Check if Node.js is installed
node --version >nul 2>&1
if %errorlevel% neq 0 (
    echo ❌ Node.js is not installed. Please install Node.js first.
    exit /b 1
)

REM Check if Rust is installed
cargo --version >nul 2>&1
if %errorlevel% neq 0 (
    echo ❌ Rust is not installed. Please install Rust first.
    exit /b 1
)

REM Install dependencies
echo 📦 Installing dependencies...
npm install

REM Build the application
echo 🔨 Building application...
npm run tauri:build

echo ✅ Build completed!
echo.
echo 📁 Built files can be found in:
echo   - MSI Installer: src-tauri\target\release\bundle\msi\
echo   - NSIS Installer: src-tauri\target\release\bundle\nsis\
echo.
echo 🎉 Ready to distribute!
