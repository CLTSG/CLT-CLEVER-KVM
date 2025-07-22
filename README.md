# Clever KVM

A remote desktop system (screen sharing, mouse and keyboard control) built with Tauri, similar to NoMachine. It uses WebSockets for real-time communication for optimal speed and display quality.

## Features

- **Web-based Remote Desktop**: Access your computer's screen from any device with a web browser
- **Real-time WebRTC Streaming**: H.264 hardware-accelerated video encoding for ultra-low latency
- **Keyboard and Mouse Control**: Full keyboard and mouse input support
- **Adaptive Quality**: Automatically adjust video quality and bitrate based on network conditions
- **Multi-codec Support**: WebRTC H.264 optimized for low latency and maximum compatibility
- **Optional Encryption**: Secure the connection between client and server
- **WebRTC Audio Support**: High-quality audio streaming from the host computer
- **Multi-monitor Support**: Select which monitor to display and control
- **Smart Frame Management**: Delta encoding and intelligent frame dropping for optimal performance
- **Auto-Updater**: Built-in automatic updates with cryptographic signature verification

## Requirements

- Rust 1.55+
- Node.js 14+
- Tauri development dependencies

### System Dependencies

#### FFmpeg (Required for all platforms)

This application requires FFmpeg for video encoding. Install it before building:

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get update
sudo apt-get install -y ffmpeg libavcodec-dev libavformat-dev libavutil-dev \
    libavdevice-dev libavfilter-dev libswscale-dev libswresample-dev pkg-config
```

**macOS:**
```bash
brew install ffmpeg pkg-config
```

**Windows:**
```bash
choco install ffmpeg pkgconfiglite -y
```

#### Tauri Dependencies

On Debian/Ubuntu-based systems, also install the Tauri dependencies:

```bash
#This is for Ubuntu 22.04 above if below this ubuntu version can skip to step number 4.
1. Open file /etc/apt/sources.list
2. Insert new line deb http://archive.ubuntu.com/ubuntu jammy main universe
3. sudo apt update
4. sudo apt install libwebkit2gtk-4.0-dev build-essential curl wget file libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libjavascriptcoregtk-4.0-bin  libjavascriptcoregtk-4.0-dev libsoup2.4-dev libxdo-dev libxcb-randr0-dev
```

#### 🩺 Troubleshooting FFmpeg Issues

If you encounter FFmpeg build errors, run our troubleshooting script:

**Linux/macOS:**
```bash
./scripts/fix-ffmpeg.sh
```

**Windows:**
```bash
scripts\fix-ffmpeg.bat
```

#### Setup and Build

1. Clone the repository:
   ```bash
   git clone https://github.com/your-username/clever-kvm.git
   cd clever-kvm
   ```

2. Install dependencies:
   ```bash
   npm install
   ```

3. Build the application:
   ```bash
   npm run tauri build
   ```

## Development

Start the development server:
```bash
npm run tauri dev
```

## Usage

1. Launch the Clever KVM application
2. Click "Start Server" to begin the KVM service
3. Use the displayed URL to access your computer from any browser on your network
4. To stop the service, click "Stop Server"

## Connection Options

The following URL parameters can be used to customize the connection:

- `stretch=true` - Stretch screen to fit window
- `mute=true` - Mute audio
- `audio=true` - Enable audio streaming
- `remoteOnly=true` - Only show remote screen (no toolbar)
- `encryption=true` - Enable encrypted connection
- `monitor=0` - Select which monitor to display (0 is primary, 1 is secondary, etc.)

```
Example: `http://hostname:9921/kvm?stretch=true;audio=true`
```

## Architecture

- **Server**: Tauri application that captures the screen and input events
- **Client**: Web-based interface accessible from any browser
- **Protocol**: WebSockets for low-latency communication with WebRTC streaming
- **Encoding**: 
  - **WebRTC H.264**: Hardware-accelerated encoding optimized for real-time performance and low latency
- **Streaming**: Real-time RTP packet delivery with adaptive bitrate control

## Logging System

Clever KVM includes a comprehensive logging system that helps with troubleshooting and monitoring:

### Log Files

- `debug.log`: Contains all log messages including debug information
- `error.log`: Contains only warning and error messages

### Viewing Logs

You can view logs directly within the application:

1. Click "Show Logs" on the main screen to access the log viewer
2. The viewer displays both error and debug logs in separate sections
3. Use the "Refresh" button to update the logs with the latest entries

### Prerequisites

- [Node.js](https://nodejs.org/) (v16 or later)
- [Rust](https://www.rust-lang.org/) (v1.67 or later)
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites)

## 📦 Building and Distribution

### Quick Build

For a quick local build:

```bash
# Make the build script executable
chmod +x scripts/build.sh

# Build the application
./scripts/build.sh
```

### Automated Releases

To create a new release with automatic GitHub deployment:

```bash
# Prepare a new release version
./scripts/prepare-release.sh 1.0.0

# Commit and tag
git add .
git commit -m "Release v1.0.0"
git tag v1.0.0
git push origin v1.0.0
```

This will automatically:
- Build for Windows, macOS, and Linux
- Create installers (.msi, .exe, .dmg, .deb, .AppImage)
- Upload to GitHub Releases

### Distribution Files

Each release includes:
- **Windows**: MSI installer and NSIS setup executable
- **macOS**: Universal DMG (Intel + Apple Silicon)
- **Linux**: Debian package and AppImage

For detailed build instructions, troubleshooting, and platform-specific notes, see [BUILD.md](BUILD.md).

## License

[MIT](LICENSE)
