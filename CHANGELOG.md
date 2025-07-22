# Clever KVM Release Changelog

## Version History

### [Unreleased]
- **🔄 Auto-Updater Implementation**: Added comprehensive auto-updater functionality
  - Automatic update detection on app startup
  - Manual update checks via UI button
  - Cryptographically signed updates for security
  - User-friendly update dialog with progress tracking
  - Background downloads with one-click installation
  - Cross-platform support (Windows, macOS, Linux)
- **🏗️ Enhanced Build System**: Improved GitHub Actions workflows with update signing
- **📚 Documentation**: Added comprehensive auto-updater documentation and guides
- **🧪 Testing Tools**: Added scripts for testing and validating updater functionality
- Initial VP8 encoding implementation
- WebSocket-based streaming
- Cross-platform desktop application
- Multi-monitor support
- Audio streaming capabilities

### [0.1.0] - 2025-07-22
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
