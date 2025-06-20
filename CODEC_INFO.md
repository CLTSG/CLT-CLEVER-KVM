# Clever KVM Codec and Streaming Technology Guide

This document outlines the video and audio technologies used in Clever KVM for high-quality, low-latency remote desktop access.

## Video Codecs

Clever KVM supports multiple video codecs to balance quality, performance, and compatibility:

### H.264 (AVC)

- **Description**: The default codec, offering good compatibility across devices and browsers
- **Advantages**: 
  - Hardware acceleration on most devices
  - Wide browser support
  - Good compression efficiency
- **Best for**: General use cases where compatibility is important

### H.265 (HEVC)

- **Description**: The successor to H.264, offering better compression
- **Advantages**:
  - 25-50% better compression than H.264 at the same quality
  - Hardware acceleration on newer devices
  - Better quality at lower bitrates
- **Best for**: Limited bandwidth scenarios or when highest quality is needed
- **Limitations**: Limited browser support, may require client-side decoding

### AV1

- **Description**: Next-generation open codec with excellent compression
- **Advantages**:
  - Superior compression efficiency (30-50% better than H.265)
  - Royalty-free
  - Increasing hardware support
- **Best for**: Future-proofing and very bandwidth-constrained scenarios
- **Limitations**: Limited hardware support, higher CPU usage for software decoding

### JPEG (Legacy)

- **Description**: Frame-by-frame JPEG compression
- **Advantages**:
  - Universal compatibility
  - Simple implementation
- **Best for**: Fallback when other codecs aren't supported
- **Limitations**: Higher bandwidth usage, lower quality

## Performance Optimizations

### Hardware Acceleration

Clever KVM automatically detects and uses hardware acceleration when available:

- **NVIDIA NVENC**: For NVIDIA GPUs
- **AMD AMF**: For AMD GPUs
- **Intel QuickSync**: For Intel CPUs with integrated graphics
- **Apple VideoToolbox**: For macOS systems

### Delta Encoding

For JPEG mode, Clever KVM implements delta encoding:

- The screen is divided into tiles (typically 64x64 pixels)
- Only tiles that have changed since the last frame are transmitted
- Significantly reduces bandwidth for static content

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
- **For H.264**: Browser with MSE (Media Source Extensions) support

### Recommended Requirements

- **Web Browser**: Latest Chrome or Edge
- **Network**: 5+ Mbps upload/download with low latency
- **For H.265/AV1**: Recent hardware for hardware decoding support

## URL Parameters

Customize the connection with these URL parameters:

- `codec=h264|h265|av1|jpeg`: Select video codec
- `monitor=0,1,2,...`: Select which monitor to display
- `audio=true`: Enable audio streaming
- `mute=true`: Connect with audio muted
- `stretch=true`: Stretch display to fit window
- `encryption=true`: Enable encrypted connection
- `remoteOnly=true`: Hide UI controls for clean display

Example: `http://hostname:9921/kvm?codec=h265;monitor=1;audio=true`

# Video Codec Support in Clever KVM

Clever KVM supports multiple video codecs for screen sharing, with different trade-offs between quality, compression efficiency, and compatibility.

## Supported Codecs

### H.264 (AVC)
- **Default codec** with wide compatibility
- Good balance of quality and compression
- Hardware acceleration available on most devices
- Works in all modern browsers

### H.265 (HEVC)
- Better compression than H.264 (up to 50% smaller files for same quality)
- Higher quality at lower bitrates
- Limited browser support
- Hardware acceleration available on newer devices

### AV1
- Newest codec with best compression efficiency
- Excellent quality at very low bitrates
- Limited hardware acceleration support
- Growing browser support

## FFmpeg Integration Notes

Clever KVM uses the `ffmpeg-next` Rust crate (version 6.1.1) for codec operations, which is a safe Rust wrapper around the FFmpeg C API. Important considerations when working with this library:

1. The API can be challenging due to FFmpeg's C-based design, requiring careful handling of mutable references and lifetimes.

2. For encoding video:
   - Create video frames in the appropriate pixel format (usually YUV420P)
   - Set up encoder with the desired codec
   - Configure parameters like bitrate, frame rate, and keyframe interval
   - Send frames to the encoder and retrieve encoded packets

3. For improved quality or performance:
   - Use hardware acceleration when available
   - Configure appropriate preset and profile settings
   - Implement adaptive bitrate based on network conditions
   - Use periodic keyframes for error resilience

## Adaptive Quality Features

Our implementation includes:

- **Frame rate control**: Limits frame rate based on performance requirements
- **Bitrate adaptation**: Dynamically adjusts bitrate based on network quality
- **Quality scaling**: Adjusts JPEG quality or video encoding parameters
- **Keyframe insertion**: Forces keyframes on network issues or periodic intervals

## Common Issues and Solutions

- **Performance bottlenecks**: Conversion between RGB and YUV can be CPU-intensive
- **Hardware acceleration compatibility**: May require specific hardware/drivers
- **Memory management**: Careful buffer management needed for high-resolution video
- **Thread safety**: Encoder operations should be properly synchronized
