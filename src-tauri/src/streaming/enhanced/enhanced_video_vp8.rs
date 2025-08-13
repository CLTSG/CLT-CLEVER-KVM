use anyhow::{Result, Context};
use thiserror::Error;
use log::{debug, error, info, warn};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::{Duration, Instant};

// VP8 encoding support
use vpx_encode::{Encoder, Config as VpxConfig, PixelFormat, Packet, Tune};
use xcap::{Monitor, XCapError};
use image::{ImageBuffer, Rgba, RgbaImage};

/// Enhanced video encoder errors
#[derive(Error, Debug)]
pub enum VideoEncoderError {
    #[error("VP8 encoder initialization failed: {0}")]
    VP8Init(String),
    #[error("Screen capture failed: {0}")]
    Capture(#[from] XCapError),
    #[error("YUV420 conversion failed: {0}")]
    YUVConversion(String),
    #[error("Encoding failed: {0}")]
    Encode(String),
    #[error("Invalid configuration: {0}")]
    Config(String),
}

/// Enhanced video configuration with YUV420 and VP8 support
#[derive(Clone, Debug)]
pub struct EnhancedVideoConfig {
    /// Target width for video output
    pub width: u32,
    /// Target height for video output
    pub height: u32,
    /// Target framerate (frames per second)
    pub framerate: u32,
    /// Target bitrate in kbps
    pub bitrate_kbps: u32,
    /// Video quality preset
    pub quality_preset: VideoQualityPreset,
    /// Enable ultra-low latency mode
    pub ultra_low_latency: bool,
    /// Enable adaptive bitrate
    pub adaptive_bitrate: bool,
    /// Maximum frame buffer size
    pub max_frame_buffer: usize,
}

/// Video quality presets optimized for screen content and YUV420
#[derive(Clone, Debug, PartialEq)]
pub enum VideoQualityPreset {
    /// Ultra-low latency, minimal compression (for gaming/interactive)
    UltraFast,
    /// Balanced quality and latency (for general screen sharing)
    Fast,
    /// Higher quality, slightly more latency (for presentations)
    Balanced,
    /// Best quality, higher latency (for video content)
    HighQuality,
    /// Custom settings
    Custom { 
        cpu_used: i32, 
        max_quantizer: u32,
        min_quantizer: u32,
    },
}

impl Default for EnhancedVideoConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            framerate: 30,
            bitrate_kbps: 3000,
            quality_preset: VideoQualityPreset::Fast,
            ultra_low_latency: true,
            adaptive_bitrate: true,
            max_frame_buffer: 5,
        }
    }
}

impl EnhancedVideoConfig {
    /// Create configuration optimized for screen sharing
    pub fn for_screen_sharing(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            framerate: 15,
            bitrate_kbps: 1500,
            quality_preset: VideoQualityPreset::Fast,
            ultra_low_latency: true,
            adaptive_bitrate: true,
            max_frame_buffer: 3,
        }
    }

    /// Create configuration optimized for gaming/interactive content
    pub fn for_gaming(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            framerate: 60,
            bitrate_kbps: 5000,
            quality_preset: VideoQualityPreset::UltraFast,
            ultra_low_latency: true,
            adaptive_bitrate: false,
            max_frame_buffer: 2,
        }
    }

    /// Create configuration optimized for high-quality video
    pub fn for_video_content(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            framerate: 30,
            bitrate_kbps: 8000,
            quality_preset: VideoQualityPreset::HighQuality,
            ultra_low_latency: false,
            adaptive_bitrate: true,
            max_frame_buffer: 10,
        }
    }
}

/// YUV420 frame data structure
#[derive(Clone)]
pub struct YUV420Frame {
    /// Y (luminance) plane
    pub y_plane: Vec<u8>,
    /// U (chroma) plane
    pub u_plane: Vec<u8>,
    /// V (chroma) plane  
    pub v_plane: Vec<u8>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Y plane stride
    pub y_stride: u32,
    /// UV plane stride
    pub uv_stride: u32,
    /// Timestamp in microseconds
    pub timestamp_us: u64,
}

impl YUV420Frame {
    /// Convert RGBA image to YUV420 format
    pub fn from_rgba_image(image: &RgbaImage, timestamp_us: u64) -> Result<Self, VideoEncoderError> {
        let width = image.width();
        let height = image.height();
        
        // YUV420 has half resolution for UV planes
        let uv_width = (width + 1) / 2;
        let uv_height = (height + 1) / 2;
        
        let y_stride = width;
        let uv_stride = uv_width;
        
        let mut y_plane = vec![0u8; (y_stride * height) as usize];
        let mut u_plane = vec![0u8; (uv_stride * uv_height) as usize];
        let mut v_plane = vec![0u8; (uv_stride * uv_height) as usize];
        
        // Convert RGB to YUV420 using standard BT.601 coefficients
        // Optimized for screen content with text and graphics
        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                let r = pixel[0] as f32;
                let g = pixel[1] as f32;
                let b = pixel[2] as f32;
                
                // YUV conversion with BT.601 coefficients (better for screen content)
                let y_val = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
                y_plane[(y * y_stride + x) as usize] = y_val;
                
                // Sample UV at half resolution (YUV420 subsampling)
                if x % 2 == 0 && y % 2 == 0 {
                    let u_val = (128.0 + (-0.147 * r - 0.289 * g + 0.436 * b)) as u8;
                    let v_val = (128.0 + (0.615 * r - 0.515 * g - 0.100 * b)) as u8;
                    
                    let uv_x = x / 2;
                    let uv_y = y / 2;
                    u_plane[(uv_y * uv_stride + uv_x) as usize] = u_val;
                    v_plane[(uv_y * uv_stride + uv_x) as usize] = v_val;
                }
            }
        }
        
        Ok(YUV420Frame {
            y_plane,
            u_plane,
            v_plane,
            width,
            height,
            y_stride,
            uv_stride,
            timestamp_us,
        })
    }
}

/// Enhanced video encoder with VP8 and YUV420 support
pub struct EnhancedVideoEncoder {
    encoder: Arc<Mutex<Option<Encoder>>>,
    config: EnhancedVideoConfig,
    frame_count: AtomicU64,
    total_bytes: AtomicU64,
    encoding_errors: AtomicU64,
    is_running: AtomicBool,
    last_keyframe_time: Arc<Mutex<Instant>>,
    adaptive_bitrate_controller: Arc<Mutex<AdaptiveBitrateController>>,
}

/// Adaptive bitrate controller for dynamic quality adjustment
struct AdaptiveBitrateController {
    target_bitrate: u32,
    current_bitrate: u32,
    recent_frame_sizes: Vec<usize>,
    last_adjustment: Instant,
}

impl AdaptiveBitrateController {
    fn new(target_bitrate: u32) -> Self {
        Self {
            target_bitrate,
            current_bitrate: target_bitrate,
            recent_frame_sizes: Vec::with_capacity(30),
            last_adjustment: Instant::now(),
        }
    }
    
    fn adjust_bitrate(&mut self, frame_size: usize, target_fps: u32) -> Option<u32> {
        self.recent_frame_sizes.push(frame_size);
        
        // Keep only recent frames (1 second worth)
        if self.recent_frame_sizes.len() > target_fps as usize {
            self.recent_frame_sizes.remove(0);
        }
        
        // Adjust every 500ms minimum
        if self.last_adjustment.elapsed() < Duration::from_millis(500) {
            return None;
        }
        
        if self.recent_frame_sizes.len() < 10 {
            return None;
        }
        
        let avg_frame_size = self.recent_frame_sizes.iter().sum::<usize>() / self.recent_frame_sizes.len();
        let current_bps = avg_frame_size * 8 * target_fps as usize;
        let current_kbps = (current_bps / 1000) as u32;
        
        let deviation = if current_kbps > self.target_bitrate {
            current_kbps - self.target_bitrate
        } else {
            self.target_bitrate - current_kbps
        };
        
        // Adjust if deviation is more than 20%
        if deviation > self.target_bitrate / 5 {
            let adjustment_factor = if current_kbps > self.target_bitrate { 0.9 } else { 1.1 };
            self.current_bitrate = ((self.current_bitrate as f32 * adjustment_factor) as u32)
                .clamp(self.target_bitrate / 4, self.target_bitrate * 2);
            
            self.last_adjustment = Instant::now();
            debug!("Adaptive bitrate: {} -> {} kbps (measured: {} kbps)", 
                   self.target_bitrate, self.current_bitrate, current_kbps);
            
            Some(self.current_bitrate)
        } else {
            None
        }
    }
}

impl EnhancedVideoEncoder {
    /// Create a new enhanced video encoder
    pub fn new(config: EnhancedVideoConfig) -> Result<Self, VideoEncoderError> {
        info!("Initializing enhanced video encoder with VP8 and YUV420 support");
        debug!("Video config: {:?}", config);
        
        let encoder = Arc::new(Mutex::new(None));
        let adaptive_controller = Arc::new(Mutex::new(
            AdaptiveBitrateController::new(config.bitrate_kbps)
        ));
        
        let encoder_instance = Self {
            encoder,
            config: config.clone(),
            frame_count: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            encoding_errors: AtomicU64::new(0),
            is_running: AtomicBool::new(false),
            last_keyframe_time: Arc::new(Mutex::new(Instant::now())),
            adaptive_bitrate_controller: adaptive_controller,
        };
        
        encoder_instance.initialize_encoder()?;
        
        Ok(encoder_instance)
    }
    
    /// Initialize the VP8 encoder with optimized settings
    fn initialize_encoder(&self) -> Result<(), VideoEncoderError> {
        let mut encoder_config = VpxConfig::new(
            self.config.width,
            self.config.height,
            Duration::from_secs(1) / self.config.framerate,
        ).context("Failed to create VP8 config")
         .map_err(|e| VideoEncoderError::VP8Init(e.to_string()))?;
        
        // Configure for YUV420 pixel format (most efficient for VP8)
        encoder_config.pixel_format = PixelFormat::I420;
        
        // Set bitrate
        encoder_config.bitrate = self.config.bitrate_kbps;
        
        // Apply quality preset optimizations
        match self.config.quality_preset {
            VideoQualityPreset::UltraFast => {
                encoder_config.tune = Tune::Screen;
                encoder_config.cpu_used = 8; // Fastest encoding
                encoder_config.max_quantizer = 56;
                encoder_config.min_quantizer = 4;
            },
            VideoQualityPreset::Fast => {
                encoder_config.tune = Tune::Screen;
                encoder_config.cpu_used = 6;
                encoder_config.max_quantizer = 48;
                encoder_config.min_quantizer = 4;
            },
            VideoQualityPreset::Balanced => {
                encoder_config.tune = Tune::Screen;
                encoder_config.cpu_used = 4;
                encoder_config.max_quantizer = 40;
                encoder_config.min_quantizer = 0;
            },
            VideoQualityPreset::HighQuality => {
                encoder_config.tune = Tune::Screen;
                encoder_config.cpu_used = 2;
                encoder_config.max_quantizer = 32;
                encoder_config.min_quantizer = 0;
            },
            VideoQualityPreset::Custom { cpu_used, max_quantizer, min_quantizer } => {
                encoder_config.tune = Tune::Screen;
                encoder_config.cpu_used = cpu_used;
                encoder_config.max_quantizer = max_quantizer;
                encoder_config.min_quantizer = min_quantizer;
            },
        }
        
        // Ultra-low latency optimizations
        if self.config.ultra_low_latency {
            encoder_config.lag_in_frames = 0; // No frame buffering
            encoder_config.error_resilient = true; // Better error recovery
            encoder_config.deadline = vpx_encode::Deadline::Realtime;
        }
        
        let encoder = Encoder::new(encoder_config)
            .context("Failed to initialize VP8 encoder")
            .map_err(|e| VideoEncoderError::VP8Init(e.to_string()))?;
        
        *self.encoder.lock() = Some(encoder);
        self.is_running.store(true, Ordering::Relaxed);
        
        info!("VP8 encoder initialized successfully - {}x{} @ {}fps, {} kbps", 
              self.config.width, self.config.height, self.config.framerate, self.config.bitrate_kbps);
        
        Ok(())
    }
    
    /// Encode a YUV420 frame to VP8
    pub fn encode_yuv420_frame(&self, yuv_frame: &YUV420Frame) -> Result<Vec<u8>, VideoEncoderError> {
        if !self.is_running.load(Ordering::Relaxed) {
            return Err(VideoEncoderError::Encode("Encoder not running".to_string()));
        }
        
        let mut encoder_guard = self.encoder.lock();
        let encoder = encoder_guard.as_mut()
            .ok_or_else(|| VideoEncoderError::Encode("Encoder not initialized".to_string()))?;
        
        // Check if we need to force a keyframe (every 2 seconds)
        let force_keyframe = {
            let mut last_keyframe = self.last_keyframe_time.lock();
            if last_keyframe.elapsed() > Duration::from_secs(2) {
                *last_keyframe = Instant::now();
                true
            } else {
                false
            }
        };
        
        // Prepare frame data in YUV420 format
        let frame_data = [
            &yuv_frame.y_plane[..],
            &yuv_frame.u_plane[..],
            &yuv_frame.v_plane[..]
        ];
        
        let strides = [
            yuv_frame.y_stride as usize,
            yuv_frame.uv_stride as usize,
            yuv_frame.uv_stride as usize,
        ];
        
        // Encode the frame
        let packets = encoder.encode_frame(&frame_data, &strides, yuv_frame.timestamp_us, force_keyframe)
            .context("VP8 encoding failed")
            .map_err(|e| {
                self.encoding_errors.fetch_add(1, Ordering::Relaxed);
                VideoEncoderError::Encode(e.to_string())
            })?;
        
        // Collect encoded data
        let mut encoded_data = Vec::new();
        for packet in packets {
            if let Packet::Data(data) = packet {
                encoded_data.extend_from_slice(&data);
            }
        }
        
        // Update statistics
        let frame_num = self.frame_count.fetch_add(1, Ordering::Relaxed);
        self.total_bytes.fetch_add(encoded_data.len() as u64, Ordering::Relaxed);
        
        // Adaptive bitrate adjustment
        if self.config.adaptive_bitrate {
            if let Some(new_bitrate) = self.adaptive_bitrate_controller.lock()
                .adjust_bitrate(encoded_data.len(), self.config.framerate) {
                
                // Note: Actual bitrate adjustment would require encoder reconfiguration
                // This is a simplified version for demonstration
                debug!("Adaptive bitrate suggests: {} kbps", new_bitrate);
            }
        }
        
        if frame_num % 100 == 0 {
            let avg_frame_size = self.total_bytes.load(Ordering::Relaxed) / (frame_num + 1);
            let error_rate = self.encoding_errors.load(Ordering::Relaxed) as f64 / (frame_num + 1) as f64;
            debug!("Encoding stats: {} frames, avg size: {} bytes, error rate: {:.2}%", 
                   frame_num + 1, avg_frame_size, error_rate * 100.0);
        }
        
        Ok(encoded_data)
    }
    
    /// Encode RGBA screen capture to VP8
    pub fn encode_screen_frame(&self, rgba_image: &RgbaImage, timestamp_us: u64) -> Result<Vec<u8>, VideoEncoderError> {
        // Convert RGBA to YUV420
        let yuv_frame = YUV420Frame::from_rgba_image(rgba_image, timestamp_us)?;
        
        // Encode YUV420 frame
        self.encode_yuv420_frame(&yuv_frame)
    }
    
    /// Get encoder statistics
    pub fn get_stats(&self) -> EncoderStats {
        let frame_count = self.frame_count.load(Ordering::Relaxed);
        let total_bytes = self.total_bytes.load(Ordering::Relaxed);
        let encoding_errors = self.encoding_errors.load(Ordering::Relaxed);
        
        EncoderStats {
            frame_count,
            total_bytes,
            encoding_errors,
            avg_frame_size: if frame_count > 0 { total_bytes / frame_count } else { 0 },
            error_rate: if frame_count > 0 { encoding_errors as f64 / frame_count as f64 } else { 0.0 },
            is_running: self.is_running.load(Ordering::Relaxed),
        }
    }
    
    /// Stop the encoder
    pub fn stop(&self) -> Result<(), VideoEncoderError> {
        info!("Stopping enhanced video encoder");
        self.is_running.store(false, Ordering::Relaxed);
        *self.encoder.lock() = None;
        Ok(())
    }
}

impl Drop for EnhancedVideoEncoder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Encoder performance statistics
#[derive(Debug, Clone)]
pub struct EncoderStats {
    pub frame_count: u64,
    pub total_bytes: u64,
    pub encoding_errors: u64,
    pub avg_frame_size: u64,
    pub error_rate: f64,
    pub is_running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    
    #[test]
    fn test_yuv420_conversion() {
        // Create a simple test image
        let width = 32;
        let height = 32;
        let mut image = ImageBuffer::new(width, height);
        
        // Fill with a gradient
        for y in 0..height {
            for x in 0..width {
                let r = (x * 255 / width) as u8;
                let g = (y * 255 / height) as u8;
                let b = 128u8;
                image.put_pixel(x, y, Rgba([r, g, b, 255]));
            }
        }
        
        let yuv_frame = YUV420Frame::from_rgba_image(&image, 0).unwrap();
        
        assert_eq!(yuv_frame.width, width);
        assert_eq!(yuv_frame.height, height);
        assert_eq!(yuv_frame.y_plane.len(), (width * height) as usize);
        assert_eq!(yuv_frame.u_plane.len(), ((width / 2) * (height / 2)) as usize);
        assert_eq!(yuv_frame.v_plane.len(), ((width / 2) * (height / 2)) as usize);
    }
}
