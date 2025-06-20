use std::time::SystemTime;
use anyhow::Result;
use ffmpeg_next as ffmpeg;
use thiserror::Error;
use log::{debug, error, info, warn};
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