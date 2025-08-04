use anyhow::Result;
use thiserror::Error;
use log::{debug, error, info, warn};
use vpx_rs::{VpxEncoder, VpxEncoderConfig, VpxInterface, VpxImage, VpxPacket, VpxRational, VpxFrameType};
use std::sync::{Arc, Mutex};
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

    pub fn to_string(&self) -> &'static str {
        match self {
            Self::VP8 => "vp8",
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

// Pure Rust VP8 VideoEncoder using vpx-rs
pub struct VideoEncoder {
    encoder: VpxEncoder,
    width: u32,
    height: u32,
    frame_index: u64,
    bitrate: u64,
    framerate: u32,
    keyframe_interval: u32,
    codec_type: CodecType,
}

impl VideoEncoder {
    pub fn new(config: EncoderConfig) -> Result<Self, CodecError> {
        info!("Initializing pure Rust {:?} encoder: {}x{} @ {}fps", 
              config.codec_type, config.width, config.height, config.framerate);
        
        // Create VP8 encoder configuration
        let mut encoder_config = VpxEncoderConfig::new()
            .map_err(|e| CodecError::Init(format!("Failed to create VPX config: {:?}", e)))?;
        
        // Set basic parameters
        encoder_config.g_w = config.width;
        encoder_config.g_h = config.height;
        encoder_config.g_timebase = VpxRational::new(1, config.framerate as i32);
        encoder_config.rc_target_bitrate = (config.bitrate / 1000) as u32; // kbps
        encoder_config.g_error_resilient = 1; // Enable error resilience
        encoder_config.g_lag_in_frames = 0; // Real-time encoding
        encoder_config.kf_max_dist = config.keyframe_interval;
        
        // Real-time encoding optimizations
        encoder_config.rc_end_usage = vpx_rs::VpxRcMode::CBR; // Constant bitrate
        encoder_config.rc_min_quantizer = 4;
        encoder_config.rc_max_quantizer = 48;
        encoder_config.rc_undershoot_pct = 95;
        encoder_config.rc_overshoot_pct = 95;
        encoder_config.rc_buf_sz = 1000;
        encoder_config.rc_buf_initial_sz = 600;
        encoder_config.rc_buf_optimal_sz = 800;
        
        // Create encoder
        let encoder = VpxEncoder::new(&encoder_config, VpxInterface::VP8)
            .map_err(|e| CodecError::Init(format!("Failed to create VP8 encoder: {:?}", e)))?;
        
        info!("VP8 encoder initialized successfully with pure Rust implementation");
        
        Ok(Self {
            encoder,
            width: config.width,
            height: config.height,
            frame_index: 0,
            bitrate: config.bitrate,
            framerate: config.framerate,
            keyframe_interval: config.keyframe_interval,
            codec_type: config.codec_type,
        })
    }

    pub fn encode_frame(&mut self, rgba_data: &[u8], force_keyframe: bool) -> Result<Vec<u8>, CodecError> {
        debug!("Encoding frame {} ({}x{}, {} bytes)", 
               self.frame_index, self.width, self.height, rgba_data.len());
        
        // Convert RGBA to YUV420P
        let yuv_data = self.rgba_to_yuv420p(rgba_data)?;
        
        // Create VPX image
        let mut image = VpxImage::new()
            .map_err(|e| CodecError::Encode(format!("Failed to create VPX image: {:?}", e)))?;
        
        image.set_img_fmt(vpx_rs::VpxImgFmt::I420);
        image.set_d_w(self.width);
        image.set_d_h(self.height);
        
        // Set YUV planes
        let y_size = (self.width * self.height) as usize;
        let uv_size = y_size / 4;
        
        image.set_planes(&[
            &yuv_data[0..y_size],                    // Y plane
            &yuv_data[y_size..y_size + uv_size],     // U plane
            &yuv_data[y_size + uv_size..],           // V plane
        ]);
        
        image.set_stride(&[
            self.width as i32,      // Y stride
            (self.width / 2) as i32, // U stride
            (self.width / 2) as i32, // V stride
        ]);
        
        // Determine frame type
        let frame_flags = if force_keyframe || self.frame_index == 0 {
            vpx_rs::VpxEncFrameFlags::KEY_FRAME
        } else {
            vpx_rs::VpxEncFrameFlags::NONE
        };
        
        // Encode frame
        self.encoder.encode(&image, self.frame_index as i64, 1, frame_flags)
            .map_err(|e| CodecError::Encode(format!("Failed to encode frame: {:?}", e)))?;
        
        // Get encoded packets
        let mut encoded_data = Vec::new();
        
        while let Ok(Some(packet)) = self.encoder.get_cx_data() {
            if let VpxPacket::Frame(frame_data) = packet {
                encoded_data.extend_from_slice(&frame_data.data);
                
                if frame_data.kind == VpxFrameType::KeyFrame {
                    debug!("Encoded keyframe {} ({} bytes)", self.frame_index, frame_data.data.len());
                }
            }
        }
        
        if encoded_data.is_empty() {
            debug!("No encoded data for frame {} (encoder may be buffering)", self.frame_index);
        } else {
            debug!("Successfully encoded frame {} ({} bytes)", self.frame_index, encoded_data.len());
        }
        
        self.frame_index += 1;
        
        Ok(encoded_data)
    }
    
    fn rgba_to_yuv420p(&self, rgba_data: &[u8]) -> Result<Vec<u8>, CodecError> {
        let width = self.width as usize;
        let height = self.height as usize;
        
        if rgba_data.len() != width * height * 4 {
            return Err(CodecError::Encode(format!(
                "Invalid RGBA data size: expected {}, got {}", 
                width * height * 4, 
                rgba_data.len()
            )));
        }
        
        let y_size = width * height;
        let uv_size = y_size / 4;
        let mut yuv_data = vec![0u8; y_size + 2 * uv_size];
        
        // Convert RGBA to YUV420P
        for y in 0..height {
            for x in 0..width {
                let rgba_idx = (y * width + x) * 4;
                let r = rgba_data[rgba_idx] as f32;
                let g = rgba_data[rgba_idx + 1] as f32;
                let b = rgba_data[rgba_idx + 2] as f32;
                
                // Y component (full resolution)
                let y_val = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
                yuv_data[y * width + x] = y_val;
                
                // U and V components (subsampled)
                if y % 2 == 0 && x % 2 == 0 {
                    let u_val = (128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b) as u8;
                    let v_val = (128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b) as u8;
                    
                    let uv_idx = (y / 2) * (width / 2) + (x / 2);
                    yuv_data[y_size + uv_idx] = u_val;                    // U plane
                    yuv_data[y_size + uv_size + uv_idx] = v_val;          // V plane
                }
            }
        }
        
        Ok(yuv_data)
    }
    
    // Add method for compatibility with existing code
    pub fn update_network_stats(&mut self, _stats: &NetworkStats) {
        // For pure Rust implementation, we could dynamically adjust encoding parameters
        // This is a placeholder for future adaptive encoding based on network conditions
    }
    
    // Add a static method to check hardware encoder availability
    pub fn is_hardware_encoder_available(_codec_type: CodecType) -> bool {
        // Pure Rust implementation doesn't use hardware encoders by default
        // This could be extended to support hardware acceleration in the future
        false
    }
}

// WebRTC-specific encoder (alias for compatibility)
pub use VideoEncoder as WebRTCVideoEncoder;

#[derive(Clone)]
pub struct WebRTCEncoderConfig {
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub framerate: u32,
    pub use_hardware: bool,
    pub keyframe_interval: u32,
    pub quality_preset: String,
}

impl Default for WebRTCEncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            bitrate: 2_000_000, // 2 Mbps
            framerate: 30,
            use_hardware: false,
            keyframe_interval: 60,
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
            use_hardware: false, // Pure Rust implementation
            keyframe_interval: 60, // Keyframe every 2 seconds at 30fps
            quality_preset: "medium".to_string(),
        }
    }

    pub fn to_encoder_config(&self) -> EncoderConfig {
        EncoderConfig {
            width: self.width,
            height: self.height,
            bitrate: self.bitrate as u64,
            framerate: self.framerate,
            keyframe_interval: self.keyframe_interval,
            preset: self.quality_preset.clone(),
            use_hardware: self.use_hardware,
            codec_type: CodecType::VP8,
        }
    }
}
