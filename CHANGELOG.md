# Clever KVM Release Changelog

## Version History

### [1.1.0] - 2025-07-22
- **🔄 Auto-Updater Implementation**: Added comprehensive auto-updater functionality
  - Automatic update detection on app startup
  - Manual update checks via UI button
  - Cryptographically signed updates for security
  - User-friendly update dialog with progress tracking
  - Background downloads with one-click installation
  - Cross-platform support (Windows, macOS, Linux)
- **🏗️ Enhanced Build System**: Improved GitHub Actions workflows with update signing
  - Fixed FFmpeg dependency installation for all platforms
  - Fixed Ubuntu linker error (libxcb-randr0-dev missing dependency)
  - Fixed macOS compilation errors (async Send trait issues, missing Key variants)
  - Fixed Windows FFmpeg build with proper vcpkg integration
  - Added comprehensive system dependency management
  - Optimized Windows builds with Chocolatey
  - Enhanced macOS builds with Homebrew integration
  - Improved Linux builds with proper FFmpeg dev libraries
- **🩺 Troubleshooting Tools**: Added FFmpeg build troubleshooting scripts
  - Cross-platform dependency checker (`fix-ffmpeg.sh` / `fix-ffmpeg.bat`)
  - Automatic FFmpeg installation and configuration
  - Environment variable setup for build success
- **📚 Documentation**: Added comprehensive auto-updater documentation and guides
  - Enhanced README with FFmpeg installation instructions
  - Detailed BUILD.md with troubleshooting guides
  - Complete pull request description with technical details
- **🧪 Testing Tools**: Added scripts for testing and validating updater functionality
 - Initial VP8 encoding implementation
 - WebSocket-based streaming
 - Cross-platform desktop application
 - Multi-monitor support
 - Audio streaming capabilities

### [1.0.0] - 2025-07-22
- Initial release
- Basic KVM functionality
- VP8 video encoding
- WebRTC audio support
- Cross-platform compatibility (Windows, macOS, Linux)

---

## Release Process

To create a new release:

1. Update version numbers using the prepare-release script:
   ```bash
   ./scripts/prepare-release.sh X.Y.Z
   ```

2. Update this CHANGELOG.md with new features and fixes

3. Commit changes:
   ```bash
   git add .
   git commit -m "Release vX.Y.Z"
   ```

4. Create and push tag:
   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

5. GitHub Actions will automatically:
   - Build for all platforms
   - Create installers
   - Create GitHub release with artifacts
