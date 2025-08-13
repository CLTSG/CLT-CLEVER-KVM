use anyhow::{Result, Context};
use thiserror::Error;
use log::{debug, error, info, warn};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::{Duration, Instant};

// Enhanced YUV420 support using existing working infrastructure
use crate::streaming::{YUV420Encoder, YUV420Config, YUV420EncoderError};
use xcap::{Monitor, XCapError};
use image::{ImageBuffer, Rgba, RgbaImage};

/// Enhanced video encoder errors
#[derive(Error, Debug)]
pub enum VideoEncoderError {
    #[error("YUV420 encoder initialization failed: {0}")]
    YUV420Init(String),
    #[error("Screen capture failed: {0}")]
    Capture(#[from] XCapError),
    #[error("YUV420 conversion failed: {0}")]
    YUVConversion(String),
    #[error("Encoding failed: {0}")]
    Encode(String),
    #[error("Invalid configuration: {0}")]
    Config(String),
}

/// Enhanced video configuration with optimized YUV420 settings
#[derive(Clone, Debug)]
pub struct EnhancedVideoConfig {
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
    pub bitrate: u32,              // Target bitrate in kbps
    pub quality: Quality,          // WebM quality setting
    pub keyframe_interval: u32,    // Keyframe every N frames
    pub monitor_id: usize,
    pub use_webm_container: bool,  // Use WebM container format
    pub enable_temporal_layers: bool, // VP8 temporal layering
    pub pixel_format: PixelFormat, // YUV420 or RGB24
}

#[derive(Clone, Debug)]
pub enum PixelFormat {
    YUV420,  // Preferred for streaming
    RGB24,   // Fallback for compatibility
    RGBA32,  // With alpha channel
}

impl Default for EnhancedVideoConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            framerate: 30,
            bitrate: 4000, // 4 Mbps
            quality: Quality::Realtime, // Good balance of quality/speed
            keyframe_interval: 90, // Every 3 seconds at 30fps
            monitor_id: 0,
            use_webm_container: true,
            enable_temporal_layers: false,
            pixel_format: PixelFormat::YUV420,
        }
    }
}

impl EnhancedVideoConfig {
    /// Configuration for high-quality WebM streaming
    pub fn webm_high_quality(monitor_id: usize) -> Self {
        Self {
            width: 1920,
            height: 1080,
            framerate: 30,
            bitrate: 8000, // 8 Mbps for excellent quality
            quality: Quality::Good, // Higher quality encoding
            keyframe_interval: 90,
            monitor_id,
            use_webm_container: true,
            enable_temporal_layers: true,
            pixel_format: PixelFormat::YUV420,
        }
    }

    /// Configuration for balanced WebM streaming
    pub fn webm_balanced(monitor_id: usize) -> Self {
        Self {
            width: 1920,
            height: 1080,
            framerate: 24,
            bitrate: 4000, // 4 Mbps balanced
            quality: Quality::Realtime, // Good speed/quality balance
            keyframe_interval: 72, // Every 3 seconds at 24fps
            monitor_id,
            use_webm_container: true,
            enable_temporal_layers: false,
            pixel_format: PixelFormat::YUV420,
        }
    }

    /// Configuration for low-latency streaming (no WebM container overhead)
    pub fn low_latency(monitor_id: usize) -> Self {
        Self {
            width: 1280,
            height: 720,
            framerate: 60,
            bitrate: 2000, // 2 Mbps for low latency
            quality: Quality::Realtime,
            keyframe_interval: 60, // Every 1 second
            monitor_id,
            use_webm_container: false, // Skip container for speed
            enable_temporal_layers: false,
            pixel_format: PixelFormat::YUV420,
        }
    }
}

/// Video frame in YUV420 format
#[derive(Debug, Clone)]
pub struct YUV420Frame {
    pub y_plane: Vec<u8>,    // Luminance
    pub u_plane: Vec<u8>,    // Chrominance U
    pub v_plane: Vec<u8>,    // Chrominance V
    pub width: u32,
    pub height: u32,
    pub timestamp: u64,      // Timestamp in microseconds
    pub frame_number: u64,
    pub is_keyframe: bool,
}

impl YUV420Frame {
    /// Create YUV420 frame from RGBA image buffer
    pub fn from_rgba(rgba_buffer: &[u8], width: u32, height: u32, timestamp: u64, frame_number: u64) -> Result<Self, VideoEncoderError> {
        let pixel_count = (width * height) as usize;
        let mut y_plane = Vec::with_capacity(pixel_count);
        let mut u_plane = Vec::with_capacity(pixel_count / 4);
        let mut v_plane = Vec::with_capacity(pixel_count / 4);

        // Convert RGBA to YUV420 using ITU-R BT.601 conversion
        for y in 0..height {
            for x in 0..width {
                let pixel_idx = ((y * width + x) * 4) as usize;
                
                if pixel_idx + 3 < rgba_buffer.len() {
                    let r = rgba_buffer[pixel_idx] as f32;
                    let g = rgba_buffer[pixel_idx + 1] as f32;
                    let b = rgba_buffer[pixel_idx + 2] as f32;
                    
                    // YUV conversion (ITU-R BT.601)
                    let y_val = (0.299 * r + 0.587 * g + 0.114 * b).round() as u8;
                    y_plane.push(y_val);
                    
                    // Subsample U and V at 2x2 blocks (4:2:0)
                    if y % 2 == 0 && x % 2 == 0 {
                        let u_val = (128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b).round() as u8;
                        let v_val = (128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b).round() as u8;
                        u_plane.push(u_val);
                        v_plane.push(v_val);
                    }
                }
            }
        }

        Ok(YUV420Frame {
            y_plane,
            u_plane,
            v_plane,
            width,
            height,
            timestamp,
            frame_number,
            is_keyframe: frame_number % 30 == 0, // Keyframe every 30 frames
        })
    }

    /// Get total frame size in bytes
    pub fn size_bytes(&self) -> usize {
        self.y_plane.len() + self.u_plane.len() + self.v_plane.len()
    }
}

/// Enhanced video encoder with WebM and YUV420 support
pub struct EnhancedVideoEncoder {
    config: EnhancedVideoConfig,
    webm_encoder: Option<Encoder>,
    monitor: Monitor,
    frame_counter: Arc<AtomicU64>,
    last_keyframe: Arc<AtomicU64>,
    encoding_stats: Arc<Mutex<EncodingStats>>,
}

#[derive(Debug, Default)]
pub struct EncodingStats {
    pub frames_encoded: u64,
    pub bytes_encoded: u64,
    pub keyframes_generated: u64,
    pub encoding_time_ms: u64,
    pub average_bitrate: f64,
    pub last_fps: f64,
}

impl EnhancedVideoEncoder {
    /// Create new enhanced video encoder
    pub fn new(config: EnhancedVideoConfig) -> Result<Self, VideoEncoderError> {
        info!("🎬 Initializing Enhanced Video Encoder with YUV420 + WebM support");
        info!("📊 Config: {}x{}@{}fps, {}kbps, WebM: {}, Format: {:?}", 
              config.width, config.height, config.framerate, config.bitrate,
              config.use_webm_container, config.pixel_format);

        // Get the specified monitor
        let monitors = Monitor::all().map_err(|e| VideoEncoderError::Capture(e))?;
        let monitor = monitors.into_iter()
            .nth(config.monitor_id)
            .ok_or_else(|| VideoEncoderError::Config(format!("Monitor {} not found", config.monitor_id)))?;

        info!("📺 Using monitor: {} ({}x{})", monitor.name(), monitor.width(), monitor.height());

        let mut encoder = Self {
            config: config.clone(),
            webm_encoder: None,
            monitor,
            frame_counter: Arc::new(AtomicU64::new(0)),
            last_keyframe: Arc::new(AtomicU64::new(0)),
            encoding_stats: Arc::new(Mutex::new(EncodingStats::default())),
        };

        // Initialize WebM encoder (we only use WebM now)
        encoder.init_webm_encoder()?;

        Ok(encoder)
    }

    /// Initialize WebM encoder with YUV420 support
    fn init_webm_encoder(&mut self) -> Result<(), VideoEncoderError> {
        info!("🎬 Initializing WebM encoder with YUV420 support");

        let encoder_config = EncoderConfig {
            width: self.config.width,
            height: self.config.height,
            quality: self.config.quality.clone(),
            speed: 6, // Good balance of speed/quality for real-time
        };

        match Encoder::new(encoder_config) {
            Ok(encoder) => {
                self.webm_encoder = Some(encoder);
                info!("✅ WebM encoder initialized successfully");
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to initialize WebM encoder: {}", e);
                Err(VideoEncoderError::WebMInit(e.to_string()))
            }
        }
    }

    /// Capture screen and encode frame
    pub async fn capture_and_encode(&mut self) -> Result<Vec<u8>, VideoEncoderError> {
        let start_time = Instant::now();
        
        // Capture screen
        let image = self.monitor.capture_image()
            .map_err(|e| VideoEncoderError::Capture(e))?;

        let frame_number = self.frame_counter.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        // Convert to YUV420 format
        let yuv_frame = match self.config.pixel_format {
            PixelFormat::YUV420 => {
                YUV420Frame::from_rgba(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    timestamp,
                    frame_number
                )?
            }
            _ => {
                return Err(VideoEncoderError::Config("Only YUV420 format supported".to_string()));
            }
        };

        // Encode frame using WebM encoder
        let encoded_data = if let Some(ref mut encoder) = self.webm_encoder {
            self.encode_webm_frame(encoder, &yuv_frame).await?
        } else {
            return Err(VideoEncoderError::Config("WebM encoder not initialized".to_string()));
        };

        // Update statistics
        let encoding_time = start_time.elapsed();
        self.update_stats(encoded_data.len(), encoding_time, yuv_frame.is_keyframe);

        Ok(encoded_data)
    }

    /// Encode YUV420 frame using WebM encoder
    async fn encode_webm_frame(&mut self, encoder: &mut Encoder, frame: &YUV420Frame) -> Result<Vec<u8>, VideoEncoderError> {
        // Prepare YUV data for WebM encoder
        let mut yuv_data = Vec::with_capacity(frame.size_bytes());
        yuv_data.extend_from_slice(&frame.y_plane);
        yuv_data.extend_from_slice(&frame.u_plane);
        yuv_data.extend_from_slice(&frame.v_plane);

        // Encode frame
        match encoder.encode_yuv(frame.width, frame.height, &yuv_data, frame.is_keyframe) {
            Ok(webm_data) => {
                debug!("🎬 WebM frame encoded: {} bytes (keyframe: {})", webm_data.len(), frame.is_keyframe);
                Ok(webm_data)
            }
            Err(e) => {
                error!("❌ WebM encoding failed: {}", e);
                Err(VideoEncoderError::Encode(e.to_string()))
            }
        }
    }

    /// Update encoding statistics
    fn update_stats(&self, encoded_bytes: usize, encoding_time: Duration, is_keyframe: bool) {
        let mut stats = self.encoding_stats.lock();
        stats.frames_encoded += 1;
        stats.bytes_encoded += encoded_bytes as u64;
        stats.encoding_time_ms += encoding_time.as_millis() as u64;
        
        if is_keyframe {
            stats.keyframes_generated += 1;
        }

        // Calculate average bitrate (last 60 frames)
        if stats.frames_encoded % 60 == 0 {
            stats.average_bitrate = (stats.bytes_encoded * 8) as f64 / (stats.encoding_time_ms as f64 / 1000.0) / 1000.0; // kbps
            stats.last_fps = 60000.0 / stats.encoding_time_ms as f64 * 60.0; // FPS based on last 60 frames
        }
    }

    /// Get current encoding statistics
    pub fn get_stats(&self) -> EncodingStats {
        self.encoding_stats.lock().clone()
    }

    /// Request keyframe on next encode
    pub fn request_keyframe(&self) {
        info!("🔑 Keyframe requested");
        // This would typically set a flag to force keyframe on next encode
    }

    /// Update encoder configuration dynamically
    pub fn update_config(&mut self, new_config: EnhancedVideoConfig) -> Result<(), VideoEncoderError> {
        info!("🔧 Updating encoder configuration");
        
        // If significant changes, reinitialize encoder
        if new_config.width != self.config.width || 
           new_config.height != self.config.height {
            
            info!("🔄 Reinitializing encoder due to significant config changes");
            self.config = new_config;
            
            // Reinitialize WebM encoder
            self.webm_encoder = None;
            self.init_webm_encoder()?;
        } else {
            // Update bitrate and other runtime parameters
            self.config = new_config;
            info!("✅ Configuration updated");
        }

        Ok(())
    }
}

impl Drop for EnhancedVideoEncoder {
    fn drop(&mut self) {
        let stats = self.get_stats();
        info!("📊 Final encoding stats: {} frames, {} MB, {:.1} kbps avg, {:.1} FPS", 
              stats.frames_encoded, 
              stats.bytes_encoded / 1024 / 1024,
              stats.average_bitrate,
              stats.last_fps);
    }
}
