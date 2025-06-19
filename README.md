# Clever KVM

A remote desktop system (screen sharing, mouse and keyboard control) built with Tauri, similar to NoMachine. It uses WebSockets for real-time communication for optimal speed and display quality.

## Features

- High-performance screen sharing
- Mouse and keyboard control
- Web-based client (access from any browser)
- Local network operation
- Configurable via URL parameters
- Optional audio streaming

## Requirements

- Rust 1.55+
- Node.js 14+
- Tauri development dependencies

### System Dependencies

On Debian/Ubuntu-based systems, install the required dependencies:

```bash
#This is for Ubuntu 22.04 above if below this ubuntu version can skip to step number 4.
1. Open file /etc/apt/sources.list
2. Insert new line deb http://archive.ubuntu.com/ubuntu jammy main universe
3. sudo apt update
4. sudo apt install libwebkit2gtk-4.0-dev build-essential curl wget file libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libjavascriptcoregtk-4.0-bin  libjavascriptcoregtk-4.0-dev libsoup2.4-dev libxdo-dev libxcb-randr0-dev
```

## Development Setup

1. Clone the repository
2. Install dependencies:
   ```
   npm install
   ```
3. Run in development mode:
   ```
   npm run tauri dev
   ```

## Building

To create a production build:

```
npm run tauri build
```

## Usage

1. Start the Clever KVM application
2. Click "Start Server" to begin hosting the KVM server
3. Access the KVM client from any device on your local network using the provided URL

### URL Parameters

You can customize the client behavior with URL parameters:

- `stretch=true` - Stretch screen to fit window
- `mute=true` - Mute audio
- `audio=true` - Enable audio streaming
- `remoteOnly=true` - Only show remote screen (no toolbar)

Example URL:
```
http://{hostname_or_IP}:9921/kvm?stretch=true;mute=true;audio=true;remoteOnly=true
```

## Architecture

- **Server**: Tauri application that captures the screen and input events
- **Client**: Web-based interface accessible from any browser
- **Protocol**: WebSockets for low-latency communication
- **Encoding**: Optimized JPEG compression for screen sharing

## License

MIT 
