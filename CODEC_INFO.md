# Clever KVM Codec and Streaming Technology Guide

This document outlines the video and audio technologies used in Clever KVM for high-quality, low-latency remote desktop access.

## Video Codec

Clever KVM uses VP8 encoding via WebRTC for optimal performance:

### WebRTC VP8

- **Description**: The primary and only codec, optimized for real-time streaming
- **Advantages**: 
  - Open-source and royalty-free
  - Universal browser support via WebRTC
  - Good compression efficiency for real-time use
  - Built-in network adaptation and error correction
  - Low-latency streaming with RTP protocol
- **Best for**: All use cases requiring real-time remote desktop access
- **Features**:
  - Adaptive bitrate based on network conditions
  - Automatic keyframe insertion for error recovery
  - Software encoding optimized for real-time performance
  - Cross-platform compatibility

## Performance Optimizations

### Adaptive Quality

The system automatically adjusts quality based on network conditions:

- Monitors latency, bandwidth, and packet loss
- Dynamically adjusts bitrate and quality settings
- Prioritizes smoothness during network congestion

## Audio Technology

### WebRTC Audio

- Uses the Opus codec for high-quality, low-latency audio
- Handles network jitter and packet loss gracefully
- Configurable quality levels (32-256 kbps)
- Support for stereo audio
- Optional echo cancellation and noise suppression

## Multi-Monitor Support

- Seamless switching between monitors
- Automatic detection of monitor configuration
- Support for HiDPI/retina displays
- Works with monitors of different resolutions

## Input Technologies

### Keyboard and Mouse

- Full keyboard mapping including special keys and international layouts
- Mouse support with wheel and multiple buttons
- Key combinations and modifier keys
- Support for keyboard shortcuts

### Touch and Gesture Support

- Multi-touch support for mobile devices
- Pinch-to-zoom gesture mapping
- Rotation gesture support
- Swipe and pan gestures

## Network Optimizations

- WebSocket protocol for low overhead
- Optional encryption for secure connections
- Adaptive frame rate based on network conditions
- Automatic reconnection on network interruption

## Client Requirements

### Minimum Requirements

- **Web Browser**: Chrome 80+, Firefox 75+, Safari 13+, Edge 80+
- **Network**: 1 Mbps upload/download
- **For VP8**: Browser with WebRTC support

### Recommended Requirements

- **Web Browser**: Latest Chrome or Edge
- **Network**: 5+ Mbps upload/download with low latency
- **For VP8**: Any modern device with WebRTC support

## URL Parameters

Customize the connection with these URL parameters:

- `codec=vp8`: Select VP8 codec (default and only option)
- `monitor=0,1,2,...`: Select which monitor to display
- `audio=true`: Enable audio streaming
- `mute=true`: Connect with audio muted
- `stretch=true`: Stretch display to fit window
- `encryption=true`: Enable encrypted connection
- `remoteOnly=true`: Hide UI controls for clean display

Example: `http://hostname:9921/kvm?codec=vp8&monitor=1&audio=true`

## FFmpeg Integration Notes

Clever KVM uses the `ffmpeg-next` Rust crate (version 6.1.1) for VP8 codec operations, which is a safe Rust wrapper around the FFmpeg C API. Important considerations when working with this library:

1. The API can be challenging due to FFmpeg's C-based design, requiring careful handling of mutable references and lifetimes.

2. For encoding video:
   - Create video frames in the appropriate pixel format (usually YUV420P)
   - Set up encoder with VP8 codec (libvpx)
   - Configure parameters like bitrate, frame rate, and keyframe interval
   - Send frames to the encoder and retrieve encoded packets

3. For improved quality or performance:
   - Use software encoding optimized for real-time streaming
   - Configure appropriate preset and profile settings
   - Implement adaptive bitrate based on network conditions
   - Use periodic keyframes for error resilience

## Adaptive Quality Features

Our implementation includes:

- **Frame rate control**: Limits frame rate based on performance requirements
- **Bitrate adaptation**: Dynamically adjusts bitrate based on network quality
- **Quality scaling**: Adjusts VP8 encoding parameters
- **Keyframe insertion**: Forces keyframes on network issues or periodic intervals

## Common Issues and Solutions

- **Performance bottlenecks**: Conversion between RGB and YUV can be CPU-intensive
- **Software encoding**: VP8 uses software encoding, optimized for real-time streaming
- **Memory management**: Careful buffer management needed for high-resolution video
- **Thread safety**: Encoder operations should be properly synchronized
