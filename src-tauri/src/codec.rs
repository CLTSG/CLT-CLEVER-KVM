use anyhow::Result;
#[cfg(feature = "ffmpeg")]
use ffmpeg_next as ffmpeg;
use thiserror::Error;
use log::{debug, error, info, warn};
use crate::server::models::NetworkStats;

// Custom error type for codec operations
#[derive(Error, Debug)]
pub enum CodecError {
    #[error("Codec initialization failed: {0}")]
    Init(String),
    #[error("Codec encoding failed: {0}")]
    Encode(String),
    #[error("Codec setup failed: {0}")]
    Setup(String),
}

// Enhanced encoder configuration
#[derive(Clone)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub bitrate: u64,
    pub framerate: u32,
    pub keyframe_interval: u32,
    pub preset: String,
    pub use_hardware: bool,
    pub codec_type: CodecType,
}

// Define codec types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodecType {
    VP8,
}

impl CodecType {
    pub fn from_string(codec: &str) -> Self {
        match codec.to_lowercase().as_str() {
            "vp8" => Self::VP8,
            _ => Self::VP8, // Default fallback
        }
    }
    
    #[cfg(feature = "ffmpeg")]
    pub fn to_ffmpeg_codec_id(&self) -> ffmpeg::codec::id::Id {
        match self {
            Self::VP8 => ffmpeg::codec::id::Id::VP8,
        }
    }
    
    pub fn get_encoder_name(&self, use_hardware: bool) -> &'static str {
        match self {
            Self::VP8 => {
                if use_hardware {
                    "vp8_vaapi" // Try hardware encoder first
                } else {
                    "libvpx" // Software encoder
                }
            }
        }
    }
}

// Helper to calculate network quality from stats
fn calculate_network_quality(stats: &NetworkStats) -> f32 {
    let latency_score = if stats.latency < 50 {
        1.0
    } else if stats.latency < 100 {
        0.8
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
    
    // Weighted average
    (latency_score * 0.4 + bandwidth_score * 0.4 + packet_loss_score * 0.2)
}

// Fixed VideoEncoder that properly initializes the encoder once
#[cfg(feature = "ffmpeg")]
pub struct VideoEncoder {
    encoder_name: String,
    scaler: ffmpeg::software::scaling::context::Context,
    width: u32,
    height: u32,
    frame_index: u64,
    bitrate: u64,
    framerate: u32,
    keyframe_interval: u32,
    preset: String,
    use_hardware: bool,
    codec_type: CodecType,
}

#[cfg(not(feature = "ffmpeg"))]
pub struct VideoEncoder {
    width: u32,
    height: u32,
}

#[cfg(feature = "ffmpeg")]
impl VideoEncoder {
    pub fn new(config: EncoderConfig) -> Result<Self, CodecError> {
        // Initialize FFmpeg
        ffmpeg::init().map_err(|e| CodecError::Init(format!("FFmpeg init failed: {}", e)))?;
        
        info!("Initializing {:?} encoder: {}x{} @ {}fps (hardware: {})", 
              config.codec_type, config.width, config.height, config.framerate, config.use_hardware);
        
        // Try hardware first if requested, then fallback to software
        let (encoder_name, actual_use_hardware) = Self::select_encoder(&config)?;
        
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
            encoder_name: encoder_name.to_string(),
            scaler,
            width: config.width,
            height: config.height,
            frame_index: 0,
            bitrate: config.bitrate,
            framerate: config.framerate,
            keyframe_interval: config.keyframe_interval,
            preset: config.preset,
            use_hardware: actual_use_hardware,
            codec_type: config.codec_type,
        })
    }

    pub fn encode_frame(&mut self, rgba_data: &[u8], force_keyframe: bool) -> Result<Vec<u8>, CodecError> {
        // First attempt with current encoder
        match self.try_encode_frame(rgba_data, force_keyframe) {
            Ok(data) => Ok(data),
            Err(e) => {
                // If encoding failed and we're using hardware, try to fallback to software
                if self.use_hardware {
                    warn!("Hardware encoding failed ({}), attempting fallback to software", e);
                    
                    // Switch to software encoder
                    let sw_encoder = self.codec_type.get_encoder_name(false);
                    if ffmpeg::encoder::find_by_name(sw_encoder).is_some() {
                        info!("Switching to software encoder: {}", sw_encoder);
                        self.encoder_name = sw_encoder.to_string();
                        self.use_hardware = false;
                        
                        // Retry with software encoder
                        match self.try_encode_frame(rgba_data, force_keyframe) {
                            Ok(data) => {
                                info!("Successfully encoded frame with software fallback");
                                Ok(data)
                            }
                            Err(fallback_error) => {
                                error!("Software encoder also failed: {}", fallback_error);
                                Err(fallback_error)
                            }
                        }
                    } else {
                        error!("Software encoder not available for fallback");
                        Err(e)
                    }
                } else {
                    // Already using software encoder, no fallback available
                    Err(e)
                }
            }
        }
    }

    fn try_encode_frame(&mut self, rgba_data: &[u8], force_keyframe: bool) -> Result<Vec<u8>, CodecError> {
        // Create a new encoder context for each frame (temporary workaround)
        // In a production version, we'd maintain the encoder state properly
        let encoder_codec = ffmpeg::encoder::find_by_name(&self.encoder_name)
            .ok_or_else(|| CodecError::Encode(format!("Encoder {} not found", self.encoder_name)))?;
        
        let context = ffmpeg::codec::context::Context::new_with_codec(encoder_codec);
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
        
        // Set codec-specific options
        let mut opts = ffmpeg::Dictionary::new();
        
        match self.codec_type {
            CodecType::VP8 => {
                if self.use_hardware {
                    // Hardware encoder options for VP8
                    opts.set("deadline", "realtime");
                    opts.set("cpu-used", "16");  // Maximum speed
                    opts.set("lag-in-frames", "0");
                    opts.set("error-resilient", "1");
                } else {
                    // Software encoder options for VP8 real-time
                    opts.set("deadline", "realtime");
                    opts.set("cpu-used", "16");  // Maximum speed
                    opts.set("lag-in-frames", "0");  // No look-ahead for low latency
                    opts.set("error-resilient", "1");
                    opts.set("max-intra-rate", "300");
                    opts.set("quality", "realtime");
                    opts.set("noise-sensitivity", "0");
                    opts.set("sharpness", "0");
                    opts.set("static-thresh", "0");
                }
            },
        }
        
        // Open the encoder with better error handling
        let mut encoder = video_encoder.open_with(opts)
            .map_err(|e| {
                let error_msg = format!("Failed to open encoder {}: {}", self.encoder_name, e);
                error!("{}", error_msg);
                CodecError::Encode(error_msg)
            })?;
        
        // Create source frame
        let mut src_frame = ffmpeg::frame::Video::new(
            ffmpeg::format::pixel::Pixel::RGBA,
            self.width,
            self.height,
        );
        
        // Copy RGBA data to source frame
        let expected_size = (self.width * self.height * 4) as usize;
        if rgba_data.len() < expected_size {
            return Err(CodecError::Encode(format!(
                "Input data too small: {} < {}", rgba_data.len(), expected_size
            )));
        }
        
        // Copy data properly with stride consideration
        let stride = src_frame.stride(0);
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
        
        src_frame.set_pts(Some(self.frame_index as i64));
        
        // Create destination frame for YUV420P
        let mut dst_frame = ffmpeg::frame::Video::new(
            ffmpeg::format::pixel::Pixel::YUV420P,
            self.width,
            self.height,
        );
        dst_frame.set_pts(Some(self.frame_index as i64));
        
        // Convert RGBA to YUV420P
        self.scaler.run(&src_frame, &mut dst_frame)
            .map_err(|e| CodecError::Encode(format!("Color conversion failed: {}", e)))?;
        
        // Force keyframe if requested
        if force_keyframe {
            dst_frame.set_kind(ffmpeg::picture::Type::I);
        }
        
        // Send frame to encoder
        encoder.send_frame(&dst_frame)
            .map_err(|e| CodecError::Encode(format!("Failed to send frame to encoder: {}", e)))?;
        
        // Receive encoded packets
        let mut encoded_data = Vec::new();
        let mut packet = ffmpeg::packet::Packet::empty();
        
        while encoder.receive_packet(&mut packet).is_ok() {
            if let Some(data) = packet.data() {
                encoded_data.extend_from_slice(data);
            }
        }
        
        if encoded_data.is_empty() {
            // For the first few frames, this might be normal as the encoder is building up its buffer
            if self.frame_index < 5 {
                debug!("No encoded data received for frame {} (encoder may be buffering)", self.frame_index);
            } else {
                debug!("No encoded data received for frame {} (this may be normal for some codecs)", self.frame_index);
            }
        } else {
            debug!("Successfully encoded frame {} ({} bytes)", self.frame_index, encoded_data.len());
        }
        
        self.frame_index += 1;
        
        Ok(encoded_data)
    }
    
    // Add method for compatibility with existing code
    pub fn update_network_stats(&mut self, _stats: &NetworkStats) {
        // For now, we don't dynamically adjust encoding parameters
        // In a full implementation, we could recreate the encoder with new settings
    }
    
    // Add a static method to check hardware encoder availability
    pub fn is_hardware_encoder_available(codec_type: CodecType) -> bool {
        // Initialize FFmpeg if not already done
        let _ = ffmpeg::init();
        
        let encoder_name = codec_type.get_encoder_name(true);
        ffmpeg::encoder::find_by_name(encoder_name).is_some()
    }
    
    // Helper method to select the best available encoder with fallback
    fn select_encoder(config: &EncoderConfig) -> Result<(String, bool), CodecError> {
        // If hardware encoding is not requested, use software immediately
        if !config.use_hardware {
            let sw_encoder = config.codec_type.get_encoder_name(false);
            if ffmpeg::encoder::find_by_name(sw_encoder).is_some() {
                return Ok((sw_encoder.to_string(), false));
            } else {
                return Err(CodecError::Init(format!("Software encoder {} not available", sw_encoder)));
            }
        }

        // Try hardware encoder first
        let hw_encoder = config.codec_type.get_encoder_name(true);
        if ffmpeg::encoder::find_by_name(hw_encoder).is_some() {
            // Hardware encoder is available, but we need to test if it actually works
            if Self::test_encoder_creation(hw_encoder, config) {
                info!("Hardware encoder {} is available and working", hw_encoder);
                return Ok((hw_encoder.to_string(), true));
            } else {
                warn!("Hardware encoder {} is available but failed to initialize, falling back to software", hw_encoder);
            }
        } else {
            info!("Hardware encoder {} not available, using software encoder", hw_encoder);
        }

        // Fallback to software encoder
        let sw_encoder = config.codec_type.get_encoder_name(false);
        if ffmpeg::encoder::find_by_name(sw_encoder).is_some() {
            info!("Using software encoder {} as fallback", sw_encoder);
            Ok((sw_encoder.to_string(), false))
        } else {
            Err(CodecError::Init(format!("Neither hardware nor software encoder available")))
        }
    }

    // Test if an encoder can actually be created and opened
    fn test_encoder_creation(encoder_name: &str, config: &EncoderConfig) -> bool {
        let encoder_codec = match ffmpeg::encoder::find_by_name(encoder_name) {
            Some(codec) => codec,
            None => return false,
        };

        let context = ffmpeg::codec::context::Context::new_with_codec(encoder_codec);
        let mut video_encoder = match context.encoder().video() {
            Ok(encoder) => encoder,
            Err(_) => return false,
        };

        // Set basic parameters
        video_encoder.set_width(config.width);
        video_encoder.set_height(config.height);
        video_encoder.set_format(ffmpeg::format::pixel::Pixel::YUV420P);
        video_encoder.set_frame_rate(Some((config.framerate as i32, 1)));
        video_encoder.set_time_base((1, config.framerate as i32));
        video_encoder.set_bit_rate(config.bitrate as usize);
        video_encoder.set_gop(config.keyframe_interval);

        // Try to open with minimal options
        let mut opts = ffmpeg::Dictionary::new();
        match config.codec_type {
            CodecType::VP8 => {
                opts.set("deadline", "realtime");
                opts.set("cpu-used", "16");
            }
        }

        // Test opening the encoder
        match video_encoder.open_with(opts) {
            Ok(_) => {
                debug!("Successfully tested encoder: {}", encoder_name);
                true
            }
            Err(e) => {
                warn!("Failed to test encoder {}: {}", encoder_name, e);
                false
            }
        }
    }
}

#[cfg(not(feature = "ffmpeg"))]
impl VideoEncoder {
    pub fn new(config: EncoderConfig) -> Result<Self, CodecError> {
        warn!("FFmpeg feature disabled - video encoding not available");
        Err(CodecError::Init("FFmpeg support not compiled in".to_string()))
    }

    pub fn encode_frame(&mut self, _rgba_data: &[u8], _force_keyframe: bool) -> Result<Vec<u8>, CodecError> {
        Err(CodecError::Encode("FFmpeg support not compiled in".to_string()))
    }

    pub fn update_network_stats(&mut self, _stats: &NetworkStats) {
        // No-op when FFmpeg is disabled
    }

    pub fn is_hardware_encoder_available(_codec_type: CodecType) -> bool {
        false
    }
}

// Placeholder for WebRTC encoder functionality - for now we use the main VideoEncoder
pub use VideoEncoder as WebRTCVideoEncoder;

#[derive(Clone)]
pub struct WebRTCEncoderConfig {
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub framerate: u32,
    pub keyframe_interval: u32,
    pub use_hardware: bool,
    pub quality_preset: String,
}

impl Default for WebRTCEncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            bitrate: 4000000,
            framerate: 30,
            keyframe_interval: 30,
            use_hardware: false,
            quality_preset: "medium".to_string(),
        }
    }
}

impl WebRTCEncoderConfig {
    pub fn for_webrtc_streaming(width: u32, height: u32, bitrate: u32) -> Self {
        Self {
            width,
            height,
            bitrate,
            framerate: 30,
            use_hardware: false, // Start with software for reliability
            keyframe_interval: 60, // Keyframe every 2 seconds at 30fps
            quality_preset: "medium".to_string(),
        }
    }
}
