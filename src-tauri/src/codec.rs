use anyhow::Result;
use ffmpeg_next as ffmpeg;
use thiserror::Error;
use log::{debug, error, info};
use crate::server::models::NetworkStats;

// Custom error type for codec operations
#[derive(Error, Debug)]
pub enum CodecError {
    #[error("Codec initialization failed: {0}")]
    Init(String),
    
    #[error("Encoding failed: {0}")]
    Encode(String),
    
    #[error("Frame rate throttling")]
    Throttling,
    
    #[error("FFmpeg error: {0}")]
    FFmpeg(#[from] ffmpeg::Error),
}

// Enhanced encoder configuration
#[derive(Clone)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub framerate: u32,
    pub keyframe_interval: u32, // In frames
    pub preset: String,
    pub use_hardware: bool,
    pub codec_type: CodecType, // Added codec type enum
}

// Define codec types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodecType {
    H264,
    H265,
    AV1,
    JPEG, // Add JPEG for fallback
}

impl CodecType {
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "h265" | "hevc" => CodecType::H265,
            "av1" => CodecType::AV1,
            "jpeg" => CodecType::JPEG,
            _ => CodecType::H264, // Default to H.264
        }
    }
    
    pub fn to_ffmpeg_codec_id(&self) -> ffmpeg::codec::Id {
        match self {
            CodecType::H264 => ffmpeg::codec::Id::H264,
            CodecType::H265 => ffmpeg::codec::Id::HEVC,
            CodecType::AV1 => ffmpeg::codec::Id::AV1,
            CodecType::JPEG => ffmpeg::codec::Id::MJPEG,
        }
    }
    
    pub fn get_encoder_name(&self, use_hardware: bool) -> &'static str {
        match (self, use_hardware) {
            (CodecType::H264, true) => "h264_nvenc",
            (CodecType::H264, false) => "libx264",
            (CodecType::H265, true) => "hevc_nvenc", 
            (CodecType::H265, false) => "libx265",
            (CodecType::AV1, true) => "av1_nvenc",
            (CodecType::AV1, false) => "libaom-av1",
            (CodecType::JPEG, _) => "mjpeg",
        }
    }
}

// Helper to calculate network quality from stats
fn calculate_network_quality(stats: &NetworkStats) -> f32 {
    // Simple quality score from 0.0 to 1.0
    let latency_score = if stats.latency < 50 {
        1.0
    } else if stats.latency < 100 {
        0.9
    } else if stats.latency < 200 {
        0.7
    } else if stats.latency < 500 {
        0.4
    } else {
        0.2
    };
    
    let bandwidth_score = if stats.bandwidth > 10.0 {
        1.0
    } else if stats.bandwidth > 5.0 {
        0.9
    } else if stats.bandwidth > 2.0 {
        0.7
    } else if stats.bandwidth > 1.0 {
        0.5
    } else {
        0.3
    };
    
    let packet_loss_score = if stats.packet_loss < 0.1 {
        1.0
    } else if stats.packet_loss < 1.0 {
        0.9
    } else if stats.packet_loss < 5.0 {
        0.6
    } else if stats.packet_loss < 10.0 {
        0.3
    } else {
        0.1
    };
    
    // Combine scores with different weights
    (latency_score * 0.3) + (bandwidth_score * 0.4) + (packet_loss_score * 0.3)
}

// Simplified encoder implementation using a different approach
pub struct VideoEncoder {
    // Store raw encoder instead of context to avoid ownership issues
    codec_id: ffmpeg::codec::Id,
    codec_name: String,
    scaler: ffmpeg::software::scaling::context::Context,
    width: u32,
    height: u32,
    frame_index: u64,
    last_keyframe: u64,
    keyframe_interval: u32,
    codec_type: CodecType,
    bitrate: u32,
    framerate: u32,
    preset: String,
    use_hardware: bool,
}

impl VideoEncoder {
    pub fn new(config: EncoderConfig) -> Result<Self, CodecError> {
        // Initialize FFmpeg
        ffmpeg::init().map_err(|e| CodecError::Init(format!("FFmpeg init failed: {}", e)))?;
        
        info!("Initializing {:?} encoder: {}x{} @ {}fps (hardware: {})", 
              config.codec_type, config.width, config.height, config.framerate, config.use_hardware);
        
        // Use software encoding by default, only try hardware if explicitly requested
        let mut encoder_name = config.codec_type.get_encoder_name(false); // Default to software
        let mut actual_use_hardware = false;
        
        // Only try hardware encoder if explicitly requested AND available
        if config.use_hardware {
            let hw_encoder_name = config.codec_type.get_encoder_name(true);
            if ffmpeg::encoder::find_by_name(hw_encoder_name).is_some() {
                info!("Hardware encoder {} available, will use it", hw_encoder_name);
                encoder_name = hw_encoder_name;
                actual_use_hardware = true;
            } else {
                info!("Hardware encoder {} not available, using software encoder {}", 
                      hw_encoder_name, encoder_name);
            }
        } else {
            info!("Using software encoder {} (hardware acceleration disabled)", encoder_name);
        }
        
        // Validate that the encoder exists
        if ffmpeg::encoder::find_by_name(encoder_name).is_none() {
            return Err(CodecError::Init(format!(
                "Encoder {} not available", 
                encoder_name
            )));
        }
        
        info!("Using encoder: {} (hardware: {})", encoder_name, actual_use_hardware);
        
        // Create scaler for RGB to YUV conversion
        let scaler = ffmpeg::software::scaling::context::Context::get(
            ffmpeg::format::pixel::Pixel::RGBA,
            config.width,
            config.height,
            ffmpeg::format::pixel::Pixel::YUV420P,
            config.width,
            config.height,
            ffmpeg::software::scaling::flag::Flags::BILINEAR,
        ).map_err(|e| CodecError::Init(format!("Failed to create scaler: {}", e)))?;
        
        info!("{:?} encoder initialized successfully", config.codec_type);
        
        Ok(Self {
            codec_id: config.codec_type.to_ffmpeg_codec_id(),
            codec_name: encoder_name.to_string(),
            scaler,
            width: config.width,
            height: config.height,
            frame_index: 0,
            last_keyframe: 0,
            keyframe_interval: config.keyframe_interval,
            codec_type: config.codec_type,
            bitrate: config.bitrate,
            framerate: config.framerate,
            preset: config.preset,
            use_hardware: actual_use_hardware,
        })
    }
    
    pub fn encode_frame(&mut self, rgba_data: &[u8], force_keyframe: bool) -> Result<Vec<u8>, CodecError> {
        // Try to create encoder context
        let encoder = ffmpeg::encoder::find_by_name(&self.codec_name)
            .ok_or_else(|| CodecError::Encode(format!("Encoder {} not found", self.codec_name)))?;
        
        let context = ffmpeg::codec::context::Context::new_with_codec(encoder);
        let mut video_encoder = context.encoder().video()
            .map_err(|e| CodecError::Encode(format!("Failed to get video encoder: {}", e)))?;
        
        // Set basic parameters
        video_encoder.set_width(self.width);
        video_encoder.set_height(self.height);
        video_encoder.set_format(ffmpeg::format::pixel::Pixel::YUV420P);
        video_encoder.set_frame_rate(Some((self.framerate as i32, 1)));
        video_encoder.set_time_base((1, self.framerate as i32));
        video_encoder.set_bit_rate(self.bitrate as usize);
        video_encoder.set_gop(self.keyframe_interval);
        
        // Set codec-specific options with better error handling
        let mut opts = ffmpeg::Dictionary::new();
        
        match self.codec_type {
            CodecType::H264 => {
                if self.use_hardware {
                    // NVENC presets
                    opts.set("preset", "fast");
                    opts.set("tune", "ll");
                    opts.set("profile", "high");
                    opts.set("level", "4.1");
                    opts.set("rc", "cbr");
                } else {
                    // Software encoder options
                    opts.set("preset", &self.preset);
                    opts.set("tune", "zerolatency");
                    opts.set("profile", "high");
                    opts.set("level", "4.1");
                    opts.set("crf", "23");
                }
            },
            CodecType::H265 => {
                if self.use_hardware {
                    opts.set("preset", "fast");
                    opts.set("tune", "ll");
                    opts.set("profile", "main");
                    opts.set("rc", "cbr");
                } else {
                    opts.set("preset", &self.preset);
                    opts.set("tune", "zerolatency");
                    opts.set("x265-params", "log-level=error");
                }
            },
            CodecType::AV1 => {
                if self.use_hardware {
                    opts.set("preset", "fast");
                    opts.set("rc", "cbr");
                } else {
                    opts.set("usage", "realtime");
                    opts.set("cpu-used", "8");
                    opts.set("tile-columns", "2");
                    opts.set("tile-rows", "1");
                }
            },
            CodecType::JPEG => {
                opts.set("q:v", "3");
            }
        }
        
        // Try to open the encoder - this is where hardware encoder issues surface
        let mut encoder = match video_encoder.open_with(opts) {
            Ok(enc) => enc,
            Err(e) => {
                // If this is a hardware encoder and it fails, we should fall back to software
                if self.use_hardware {
                    return Err(CodecError::Encode(format!(
                        "Hardware encoder failed to open ({}). Hardware acceleration may not be available.", e
                    )));
                } else {
                    return Err(CodecError::Encode(format!("Software encoder failed to open: {}", e)));
                }
            }
        };
        
        // Create source frame
        let mut src_frame = ffmpeg::frame::Video::new(
            ffmpeg::format::pixel::Pixel::RGBA,
            self.width,
            self.height,
        );
        
        // Copy RGBA data to frame
        let expected_size = (self.width * self.height * 4) as usize;
        if rgba_data.len() < expected_size {
            return Err(CodecError::Encode(format!(
                "Input data too small: {} < {}", rgba_data.len(), expected_size
            )));
        }
        
        // Get stride before mutable borrow to avoid borrowing conflicts
        let stride = src_frame.stride(0);
        
        // Get frame data pointer and copy
        let frame_data = src_frame.data_mut(0);
        
        for y in 0..self.height as usize {
            let src_offset = y * self.width as usize * 4;
            let dst_offset = y * stride;
            let row_size = self.width as usize * 4;
            
            if dst_offset + row_size <= frame_data.len() && src_offset + row_size <= rgba_data.len() {
                frame_data[dst_offset..dst_offset + row_size]
                    .copy_from_slice(&rgba_data[src_offset..src_offset + row_size]);
            }
        }
        
        // Create destination frame for YUV
        let mut dst_frame = ffmpeg::frame::Video::new(
            ffmpeg::format::pixel::Pixel::YUV420P,
            self.width,
            self.height,
        );
        
        // Scale/convert RGBA to YUV420P
        self.scaler.run(&src_frame, &mut dst_frame)
            .map_err(|e| CodecError::Encode(format!("Scaling failed: {}", e)))?;
        
        // Set frame timestamp
        dst_frame.set_pts(Some(self.frame_index as i64));
        
        // Force keyframe if needed
        let need_keyframe = force_keyframe || 
            (self.frame_index - self.last_keyframe) >= self.keyframe_interval as u64;
            
        if need_keyframe {
            dst_frame.set_kind(ffmpeg::picture::Type::I);
            self.last_keyframe = self.frame_index;
            debug!("Keyframe at frame {}", self.frame_index);
        }
        
        self.frame_index += 1;
        
        // Send frame to encoder
        encoder.send_frame(&dst_frame)
            .map_err(|e| CodecError::Encode(format!("Send frame failed: {}", e)))?;
        
        // Receive encoded packets
        let mut encoded_data = Vec::new();
        let mut packet = ffmpeg::packet::Packet::empty();
        
        while encoder.receive_packet(&mut packet).is_ok() {
            if let Some(data) = packet.data() {
                encoded_data.extend_from_slice(data);
            }
        }
        
        if encoded_data.is_empty() {
            return Err(CodecError::Encode("No data encoded".to_string()));
        }
        
        Ok(encoded_data)
    }
    
    pub fn flush(&mut self) -> Result<Vec<u8>, CodecError> {
        // For now, return empty data as flushing requires maintaining encoder state
        // In a production implementation, we'd need to maintain the encoder context
        // across multiple encode_frame calls
        Ok(Vec::new())
    }
    
    pub fn update_network_stats(&mut self, stats: &NetworkStats) {
        // Simple adaptive bitrate based on network conditions
        let quality_score = calculate_network_quality(stats);
        
        if quality_score < 0.5 && self.frame_index % 30 == 0 {
            debug!("Poor network quality ({:.2}), consider reducing bitrate", quality_score);
            
            // Reduce bitrate for poor network conditions
            if self.bitrate > 1_000_000 {
                self.bitrate = (self.bitrate as f32 * 0.8) as u32;
                debug!("Reduced bitrate to {} kbps", self.bitrate / 1000);
            }
        } else if quality_score > 0.8 && self.frame_index % 60 == 0 {
            // Increase bitrate for good network conditions
            if self.bitrate < 8_000_000 {
                self.bitrate = (self.bitrate as f32 * 1.1) as u32;
                debug!("Increased bitrate to {} kbps", self.bitrate / 1000);
            }
        }
    }
    
    // Add a static method to check hardware encoder availability
    pub fn is_hardware_encoder_available(codec_type: CodecType) -> bool {
        // Initialize FFmpeg if not already done
        let _ = ffmpeg::init();
        
        let encoder_name = codec_type.get_encoder_name(true);
        ffmpeg::encoder::find_by_name(encoder_name).is_some()
    }
    
    // Add a method to get supported presets for a codec type
    pub fn get_supported_presets(codec_type: CodecType, use_hardware: bool) -> Vec<&'static str> {
        match (codec_type, use_hardware) {
            (CodecType::H264, true) => vec!["fast", "medium", "slow", "hp", "hq", "ll", "llhq", "llhp"],
            (CodecType::H264, false) => vec!["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"],
            (CodecType::H265, true) => vec!["fast", "medium", "slow", "hp", "hq", "ll", "llhq", "llhp"],
            (CodecType::H265, false) => vec!["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"],
            (CodecType::AV1, true) => vec!["fast", "medium", "slow"],
            (CodecType::AV1, false) => vec!["realtime"],
            (CodecType::JPEG, _) => vec!["default"],
        }
    }
}

use std::sync::mpsc;
use std::thread;

// Add WebRTC-specific encoder configuration
#[derive(Clone)]
pub struct WebRTCEncoderConfig {
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub framerate: u32,
    pub profile: H264Profile,
    pub level: H264Level,
    pub tune: H264Tune,
    pub preset: H264Preset,
    pub use_hardware: bool,
    pub low_latency: bool,
    pub slice_mode: SliceMode,
}

#[derive(Debug, Clone, Copy)]
pub enum H264Profile {
    Baseline,
    Main,
    High,
    ConstrainedBaseline,
}

#[derive(Debug, Clone, Copy)]
pub enum H264Level {
    Level31,
    Level32,
    Level40,
    Level41,
    Level42,
    Level50,
    Level51,
}

#[derive(Debug, Clone, Copy)]
pub enum H264Tune {
    ZeroLatency,
    FastDecode,
    Film,
    Animation,
    StillImage,
}

#[derive(Debug, Clone, Copy)]
pub enum H264Preset {
    UltraFast,
    SuperFast,
    VeryFast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    VerySlow,
}

#[derive(Debug, Clone, Copy)]
pub enum SliceMode {
    Single,
    Fixed(u32),    // Fixed number of slices
    MaxSize(u32),  // Maximum bytes per slice
}

impl WebRTCEncoderConfig {
    pub fn for_webrtc_streaming(width: u32, height: u32, bitrate: u32) -> Self {
        Self {
            width,
            height,
            bitrate,
            framerate: 30,
            profile: H264Profile::ConstrainedBaseline, // Best WebRTC compatibility
            level: H264Level::Level31,
            tune: H264Tune::ZeroLatency,
            preset: H264Preset::UltraFast,
            use_hardware: false, // Start with software for reliability
            low_latency: true,
            slice_mode: SliceMode::Fixed(4), // Multiple slices for better error resilience
        }
    }
}

// Enhanced video encoder for WebRTC
pub struct WebRTCVideoEncoder {
    encoder_tx: mpsc::Sender<EncoderCommand>,
    output_rx: mpsc::Receiver<EncodedFrame>,
    _encoder_thread: thread::JoinHandle<()>,
}

#[derive(Debug)]
enum EncoderCommand {
    EncodeFrame { data: Vec<u8>, force_keyframe: bool },
    UpdateBitrate(u32),
    RequestKeyframe,
    Shutdown,
}

#[derive(Debug)]
pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub timestamp: u64,
    pub sequence_number: u32,
    pub nal_units: Vec<NalUnit>,
}

#[derive(Debug)]
pub struct NalUnit {
    pub nal_type: u8,
    pub data: Vec<u8>,
    pub start_code_length: usize,
}

impl WebRTCVideoEncoder {
    pub fn new(config: WebRTCEncoderConfig) -> Result<Self, CodecError> {
        let (encoder_tx, encoder_rx) = mpsc::channel();
        let (output_tx, output_rx) = mpsc::channel();
        
        let encoder_thread = thread::spawn(move || {
            Self::encoder_thread(config, encoder_rx, output_tx);
        });
        
        Ok(Self {
            encoder_tx,
            output_rx,
            _encoder_thread: encoder_thread,
        })
    }
    
    fn encoder_thread(
        config: WebRTCEncoderConfig,
        encoder_rx: mpsc::Receiver<EncoderCommand>,
        output_tx: mpsc::Sender<EncodedFrame>,
    ) {
        // Initialize FFmpeg
        if let Err(e) = ffmpeg::init() {
            error!("Failed to initialize FFmpeg: {}", e);
            return;
        }
        
        // Create encoder with WebRTC-optimized settings
        let encoder_name = if config.use_hardware {
            "h264_nvenc"
        } else {
            "libx264"
        };
        
        let encoder = match ffmpeg::encoder::find_by_name(encoder_name) {
            Some(enc) => enc,
            None => {
                error!("Encoder {} not found", encoder_name);
                return;
            }
        };
        
        let context = ffmpeg::codec::context::Context::new_with_codec(encoder);
        let mut video_encoder = match context.encoder().video() {
            Ok(enc) => enc,
            Err(e) => {
                error!("Failed to create video encoder: {}", e);
                return;
            }
        };
        
        // Configure encoder for WebRTC
        video_encoder.set_width(config.width);
        video_encoder.set_height(config.height);
        video_encoder.set_format(ffmpeg::format::pixel::Pixel::YUV420P);
        video_encoder.set_frame_rate(Some((config.framerate as i32, 1)));
        video_encoder.set_time_base((1, config.framerate as i32));
        video_encoder.set_bit_rate(config.bitrate as usize);
        
        // WebRTC-specific options
        let mut opts = ffmpeg::Dictionary::new();
        
        if config.use_hardware {
            // NVENC settings for WebRTC
            opts.set("preset", "p1");  // Ultra-low latency preset
            opts.set("tune", "ll");    // Low latency tune
            opts.set("profile", "baseline");
            opts.set("level", "3.1");
            opts.set("rc", "cbr");     // Constant bitrate for WebRTC
            opts.set("zerolatency", "1");
            opts.set("forced-idr", "1");
            opts.set("slice-mode", "4"); // Fixed number of slices
            opts.set("slices", "4");
        } else {
            // Software encoder settings for WebRTC
            opts.set("preset", "ultrafast");
            opts.set("tune", "zerolatency");
            opts.set("profile", "baseline");
            opts.set("level", "3.1");
            opts.set("keyint", "60");  // Keyframe every 2 seconds at 30fps
            opts.set("keyint_min", "30");
            opts.set("scenecut", "0"); // Disable scene cut detection
            opts.set("bframes", "0");  // No B-frames for low latency
            opts.set("slices", "4");   // Multiple slices for error resilience
            opts.set("slice-max-size", "1200"); // MTU-friendly slice sizes
            opts.set("nal-hrd", "cbr");
            opts.set("force-cfr", "1");
            opts.set("repeat-headers", "1"); // Include SPS/PPS in every keyframe
        }
        
        let mut encoder = match video_encoder.open_with(opts) {
            Ok(enc) => enc,
            Err(e) => {
                error!("Failed to open encoder: {}", e);
                return;
            }
        };
        
        // Create scaler for RGB to YUV conversion
        let mut scaler = match ffmpeg::software::scaling::context::Context::get(
            ffmpeg::format::pixel::Pixel::RGBA,
            config.width,
            config.height,
            ffmpeg::format::pixel::Pixel::YUV420P,
            config.width,
            config.height,
            ffmpeg::software::scaling::flag::Flags::BILINEAR,
        ) {
            Ok(scaler) => scaler,
            Err(e) => {
                error!("Failed to create scaler: {}", e);
                return;
            }
        };
        
        let mut frame_count = 0u32;
        let mut last_keyframe = 0u32;
        
        // Encoder loop
        while let Ok(command) = encoder_rx.recv() {
            match command {
                EncoderCommand::EncodeFrame { data, force_keyframe } => {
                    if let Ok(encoded_frame) = Self::encode_frame_internal(
                        &mut encoder,
                        &mut scaler,
                        &data,
                        config.width,
                        config.height,
                        frame_count,
                        force_keyframe || (frame_count - last_keyframe) >= 60,
                    ) {
                        if encoded_frame.is_keyframe {
                            last_keyframe = frame_count;
                        }
                        
                        if output_tx.send(encoded_frame).is_err() {
                            break;
                        }
                    }
                    frame_count += 1;
                },
                EncoderCommand::UpdateBitrate(new_bitrate) => {
                    // Dynamic bitrate adjustment
                    info!("Updating bitrate to {} kbps", new_bitrate / 1000);
                    // Note: This would require recreating the encoder context
                    // For now, we'll log it and implement in future versions
                },
                EncoderCommand::RequestKeyframe => {
                    // Force next frame to be a keyframe
                    // This would be implemented in the next encode cycle
                },
                EncoderCommand::Shutdown => {
                    break;
                }
            }
        }
        
        info!("Encoder thread shutting down");
    }
    
    fn encode_frame_internal(
        encoder: &mut ffmpeg::encoder::video::Video,
        scaler: &mut ffmpeg::software::scaling::context::Context,
        rgba_data: &[u8],
        width: u32,
        height: u32,
        frame_number: u32,
        force_keyframe: bool,
    ) -> Result<EncodedFrame, CodecError> {
        // Create source frame
        let mut src_frame = ffmpeg::frame::Video::new(
            ffmpeg::format::pixel::Pixel::RGBA,
            width,
            height,
        );
        
        // Copy RGBA data to frame
        let expected_size = (width * height * 4) as usize;
        if rgba_data.len() < expected_size {
            return Err(CodecError::Encode(format!(
                "Input data too small: {} < {}", rgba_data.len(), expected_size
            )));
        }
        
        let stride = src_frame.stride(0);
        let frame_data = src_frame.data_mut(0);
        
        for y in 0..height as usize {
            let src_offset = y * width as usize * 4;
            let dst_offset = y * stride;
            let row_size = width as usize * 4;
            
            if dst_offset + row_size <= frame_data.len() && src_offset + row_size <= rgba_data.len() {
                frame_data[dst_offset..dst_offset + row_size]
                    .copy_from_slice(&rgba_data[src_offset..src_offset + row_size]);
            }
        }
        
        // Create destination frame for YUV
        let mut dst_frame = ffmpeg::frame::Video::new(
            ffmpeg::format::pixel::Pixel::YUV420P,
            width,
            height,
        );
        
        // Scale/convert RGBA to YUV420P
        scaler.run(&src_frame, &mut dst_frame)
            .map_err(|e| CodecError::Encode(format!("Scaling failed: {}", e)))?;
        
        // Set frame timestamp and properties
        dst_frame.set_pts(Some(frame_number as i64));
        
        if force_keyframe {
            dst_frame.set_kind(ffmpeg::picture::Type::I);
        }
        
        // Send frame to encoder
        encoder.send_frame(&dst_frame)
            .map_err(|e| CodecError::Encode(format!("Send frame failed: {}", e)))?;
        
        // Receive encoded packets
        let mut encoded_data = Vec::new();
        let mut packet = ffmpeg::packet::Packet::empty();
        let mut is_keyframe = false;
        
        while encoder.receive_packet(&mut packet).is_ok() {
            if let Some(data) = packet.data() {
                encoded_data.extend_from_slice(data);
                is_keyframe = packet.flags().intersects(ffmpeg::packet::flag::Flags::KEY);
            }
        }
        
        if encoded_data.is_empty() {
            return Err(CodecError::Encode("No data encoded".to_string()));
        }
        
        // Parse NAL units for WebRTC
        let nal_units = Self::parse_nal_units(&encoded_data);
        
        Ok(EncodedFrame {
            data: encoded_data,
            is_keyframe,
            timestamp: frame_number as u64,
            sequence_number: frame_number,
            nal_units,
        })
    }
    
    fn parse_nal_units(data: &[u8]) -> Vec<NalUnit> {
        let mut nal_units = Vec::new();
        let mut i = 0;
        
        while i < data.len() {
            // Look for start code (0x000001 or 0x00000001)
            if i + 3 < data.len() && data[i] == 0x00 && data[i + 1] == 0x00 {
                let start_code_length = if data[i + 2] == 0x00 && data[i + 3] == 0x01 {
                    4 // 0x00000001
                } else if data[i + 2] == 0x01 {
                    3 // 0x000001
                } else {
                    i += 1;
                    continue;
                };
                
                // Find next start code or end of data
                let mut end = i + start_code_length;
                while end + 3 < data.len() {
                    if data[end] == 0x00 && data[end + 1] == 0x00 &&
                       (data[end + 2] == 0x01 || (data[end + 2] == 0x00 && data[end + 3] == 0x01)) {
                        break;
                    }
                    end += 1;
                }
                
                if end == i + start_code_length {
                    i += start_code_length;
                    continue;
                }
                
                // Extract NAL unit
                let nal_start = i + start_code_length;
                let nal_data = data[nal_start..end].to_vec();
                
                if !nal_data.is_empty() {
                    let nal_type = nal_data[0] & 0x1F;
                    nal_units.push(NalUnit {
                        nal_type,
                        data: nal_data,
                        start_code_length,
                    });
                }
                
                i = end;
            } else {
                i += 1;
            }
        }
        
        nal_units
    }
    
    pub fn encode_frame(&self, rgba_data: Vec<u8>, force_keyframe: bool) -> Result<(), CodecError> {
        self.encoder_tx.send(EncoderCommand::EncodeFrame {
            data: rgba_data,
            force_keyframe,
        }).map_err(|e| CodecError::Encode(format!("Failed to send encode command: {}", e)))?;
        
        Ok(())
    }
    
    pub fn get_encoded_frame(&self) -> Option<EncodedFrame> {
        self.output_rx.try_recv().ok()
    }
    
    pub fn update_bitrate(&self, bitrate: u32) -> Result<(), CodecError> {
        self.encoder_tx.send(EncoderCommand::UpdateBitrate(bitrate))
            .map_err(|e| CodecError::Encode(format!("Failed to send bitrate update: {}", e)))?;
        Ok(())
    }
    
    pub fn request_keyframe(&self) -> Result<(), CodecError> {
        self.encoder_tx.send(EncoderCommand::RequestKeyframe)
            .map_err(|e| CodecError::Encode(format!("Failed to send keyframe request: {}", e)))?;
        Ok(())
    }
}

// WebRTC-specific utilities
pub struct WebRTCUtils;

impl WebRTCUtils {
    pub fn create_h264_sdp(width: u32, height: u32, profile_level_id: &str) -> String {
        format!(
            "v=0\r\n\
            o=- 0 0 IN IP4 127.0.0.1\r\n\
            s=Clever KVM\r\n\
            c=IN IP4 127.0.0.1\r\n\
            t=0 0\r\n\
            m=video 9 RTP/AVP 96\r\n\
            a=rtpmap:96 H264/90000\r\n\
            a=fmtp:96 profile-level-id={}; packetization-mode=1; sprop-parameter-sets=\r\n\
            a=framerate:30\r\n\
            a=framesize:96 {}-{}\r\n\
            a=sendonly\r\n",
            profile_level_id, width, height
        )
    }
    
    pub fn extract_sps_pps(nal_units: &[NalUnit]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        let mut sps = None;
        let mut pps = None;
        
        for nal_unit in nal_units {
            match nal_unit.nal_type {
                7 => sps = Some(nal_unit.data.clone()), // SPS
                8 => pps = Some(nal_unit.data.clone()), // PPS
                _ => {}
            }
        }
        
        (sps, pps)
    }
    
    pub fn create_rtp_packet(
        nal_unit: &NalUnit,
        sequence_number: u16,
        timestamp: u32,
        ssrc: u32,
        max_payload_size: usize,
    ) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        let payload_header_size = 12; // RTP header size
        let max_nal_size = max_payload_size - payload_header_size;
        
        if nal_unit.data.len() <= max_nal_size {
            // Single NAL unit packet
            let mut packet = Self::create_rtp_header(sequence_number, timestamp, ssrc);
            packet.extend_from_slice(&nal_unit.data);
            packets.push(packet);
        } else {
            // Fragment into FU-A packets
            let nal_header = nal_unit.data[0];
            let payload = &nal_unit.data[1..];
            let fu_indicator = (nal_header & 0xE0) | 28; // FU-A type
            let fu_header_start = 0x80 | (nal_header & 0x1F); // Start bit + NAL type
            
            let fragment_size = max_nal_size - 2; // Account for FU indicator and header
            let total_fragments = (payload.len() + fragment_size - 1) / fragment_size;
            
            for (i, chunk) in payload.chunks(fragment_size).enumerate() {
                let mut fu_header = if i == 0 {
                    fu_header_start
                } else if i == total_fragments - 1 {
                    0x40 | (nal_header & 0x1F) // End bit + NAL type
                } else {
                    nal_header & 0x1F // NAL type only
                };
                
                let mut packet = Self::create_rtp_header(
                    sequence_number.wrapping_add(i as u16),
                    timestamp,
                    ssrc,
                );
                packet.push(fu_indicator);
                packet.push(fu_header);
                packet.extend_from_slice(chunk);
                packets.push(packet);
            }
        }
        
        packets
    }
    
    fn create_rtp_header(sequence_number: u16, timestamp: u32, ssrc: u32) -> Vec<u8> {
        let mut header = vec![0u8; 12];
        
        // Version (2 bits) = 2, Padding (1 bit) = 0, Extension (1 bit) = 0, 
        // CSRC count (4 bits) = 0
        header[0] = 0x80;
        
        // Marker (1 bit) = 0, Payload type (7 bits) = 96 (H.264)
        header[1] = 96;
        
        // Sequence number (16 bits)
        header[2..4].copy_from_slice(&sequence_number.to_be_bytes());
        
        // Timestamp (32 bits)
        header[4..8].copy_from_slice(&timestamp.to_be_bytes());
        
        // SSRC (32 bits)
        header[8..12].copy_from_slice(&ssrc.to_be_bytes());
        
        header
    }
}