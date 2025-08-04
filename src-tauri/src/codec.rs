use anyhow::Result;
use thiserror::Error;
use log::{debug, error, info, warn};
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

// Simple VP8 VideoEncoder using a basic implementation
pub struct VideoEncoder {
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
        info!("Initializing software {:?} encoder: {}x{} @ {}fps", 
              config.codec_type, config.width, config.height, config.framerate);
        
        // For now, we'll implement a basic software encoder
        // In a production environment, you would integrate with a proper VP8 library
        
        info!("Software VP8 encoder initialized successfully");
        
        Ok(Self {
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
        
        // For this demo, we'll implement a basic frame compression
        // In production, this would use a proper VP8 encoder
        let compressed_data = self.basic_compress(rgba_data, force_keyframe)?;
        
        debug!("Successfully encoded frame {} ({} bytes)", self.frame_index, compressed_data.len());
        
        self.frame_index += 1;
        
        Ok(compressed_data)
    }
    
    fn basic_compress(&self, rgba_data: &[u8], is_keyframe: bool) -> Result<Vec<u8>, CodecError> {
        // This is a placeholder implementation
        // In production, you would use a proper VP8 encoder library
        
        let width = self.width as usize;
        let height = self.height as usize;
        
        if rgba_data.len() != width * height * 4 {
            return Err(CodecError::Encode(format!(
                "Invalid RGBA data size: expected {}, got {}", 
                width * height * 4, 
                rgba_data.len()
            )));
        }
        
        // Simple compression: downsample and reduce color depth
        let mut compressed = Vec::new();
        
        // VP8 header simulation (simplified)
        if is_keyframe {
            compressed.extend_from_slice(&[0x10, 0x02, 0x00]); // Keyframe header
        } else {
            compressed.extend_from_slice(&[0x30, 0x02, 0x00]); // Inter-frame header
        }
        
        // Add frame dimensions
        compressed.extend_from_slice(&(width as u16).to_le_bytes());
        compressed.extend_from_slice(&(height as u16).to_le_bytes());
        
        // Basic compression: subsample every 4th pixel for demonstration
        for y in (0..height).step_by(2) {
            for x in (0..width).step_by(2) {
                let idx = (y * width + x) * 4;
                if idx + 3 < rgba_data.len() {
                    // Convert RGB to YUV and compress
                    let r = rgba_data[idx] as f32;
                    let g = rgba_data[idx + 1] as f32;
                    let b = rgba_data[idx + 2] as f32;
                    
                    let y_val = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
                    compressed.push(y_val);
                    
                    // Add U and V components less frequently
                    if x % 4 == 0 && y % 4 == 0 {
                        let u_val = (128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b) as u8;
                        let v_val = (128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b) as u8;
                        compressed.push(u_val);
                        compressed.push(v_val);
                    }
                }
            }
        }
        
        // Apply simple RLE compression
        let mut final_compressed = Vec::new();
        let mut i = 0;
        while i < compressed.len() {
            let current = compressed[i];
            let mut count = 1u8;
            
            // Count consecutive identical bytes
            while i + (count as usize) < compressed.len() && 
                  compressed[i + (count as usize)] == current && 
                  count < 255 {
                count += 1;
            }
            
            if count > 3 {
                // Use RLE for runs > 3
                final_compressed.push(0xFF); // RLE marker
                final_compressed.push(count);
                final_compressed.push(current);
            } else {
                // Copy bytes directly
                for _ in 0..count {
                    final_compressed.push(current);
                }
            }
            
            i += count as usize;
        }
        
        Ok(final_compressed)
    }
    
    // Add method for compatibility with existing code
    pub fn update_network_stats(&mut self, _stats: &NetworkStats) {
        // For basic implementation, we could adjust compression quality
        // This is a placeholder for future adaptive encoding
    }
    
    // Add a static method to check hardware encoder availability
    pub fn is_hardware_encoder_available(_codec_type: CodecType) -> bool {
        // Software implementation doesn't use hardware encoders
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
