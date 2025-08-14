use scap::{
    capturer::{Capturer, Options, Resolution},
    frame::{Frame, FrameType},
    is_supported, has_permission, request_permission
};
use log::{info, warn};
use std::sync::Mutex;

// For delta encoding
#[derive(Clone)]
pub struct ScreenTile {
    pub data: Vec<u8>,
    pub hash: u64,
    pub changed: bool,
}

pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub width: usize,
    pub height: usize,
    pub position_x: i32,
    pub position_y: i32,
    pub scale_factor: f64,  // Added for HiDPI displays
    pub rotation: i32,      // 0, 90, 180, 270 degrees
    pub supports_cursor: bool, // New: scap cursor support
    pub supports_highlight: bool, // New: scap highlight support
}

pub struct ScreenCapture {
    capturer: Option<Capturer>,
    width: usize,
    height: usize,
    tile_size: usize,
    tiles: Vec<ScreenTile>,
    previous_frame: Option<Vec<u8>>,
    // Track quality based on network conditions
    adaptive_quality: Mutex<u8>,
    // Monitor info
    monitor_id: String,
    is_primary: bool,
    // Enhanced features (native scap support)
    show_cursor: bool,
    output_format: OutputFormat,
    capture_fps: u32,
    // YUV conversion support
    yuv_converter: Option<YuvConverter>,
}

// Output format enum to prepare for scap migration
#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    RGBA,
    BGRA,
    YUV420, // Simulated YUV support
}

// YUV converter for future scap compatibility
pub struct YuvConverter {
    width: usize,
    height: usize,
}

impl YuvConverter {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }
    
    // Convert RGBA to YUV420 (simplified implementation)
    pub fn rgba_to_yuv420(&self, rgba_data: &[u8]) -> Vec<u8> {
        let pixel_count = self.width * self.height;
        let mut yuv_data = Vec::with_capacity(pixel_count * 3 / 2); // Y + U/2 + V/2
        
        // Y plane
        for chunk in rgba_data.chunks_exact(4) {
            let r = chunk[0] as f32;
            let g = chunk[1] as f32;
            let b = chunk[2] as f32;
            
            // ITU-R BT.601 conversion
            let y = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
            yuv_data.push(y);
        }
        
        // U and V planes (subsampled 4:2:0)
        for y in (0..self.height).step_by(2) {
            for x in (0..self.width).step_by(2) {
                let idx = (y * self.width + x) * 4;
                if idx + 4 <= rgba_data.len() {
                    let r = rgba_data[idx] as f32;
                    let g = rgba_data[idx + 1] as f32;
                    let b = rgba_data[idx + 2] as f32;
                    
                    let u = (-0.169 * r - 0.331 * g + 0.500 * b + 128.0) as u8;
                    let v = (0.500 * r - 0.419 * g - 0.081 * b + 128.0) as u8;
                    
                    yuv_data.push(u);
                    yuv_data.push(v);
                }
            }
        }
        
        yuv_data
    }
}

impl ScreenCapture {
    // Getter methods for private fields
    pub fn width(&self) -> usize {
        self.width
    }
    
    pub fn height(&self) -> usize {
        self.height
    }

    pub fn new(monitor_index: Option<usize>) -> Result<Self, Box<dyn std::error::Error>> {
        Self::new_with_options(monitor_index, true, OutputFormat::RGBA)
    }
    
    // Enhanced constructor with cursor and format options (using scap)
    pub fn new_with_options(
        monitor_index: Option<usize>, 
        show_cursor: bool, 
        output_format: OutputFormat
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Check if the platform is supported
        if !is_supported() {
            return Err("❌ Platform not supported".into());
        }

        // Check if we have permission to capture screen
        if !has_permission() {
            warn!("❌ Permission not granted. Requesting permission...");
            if !request_permission() {
                return Err("❌ Permission denied".into());
            }
        }

        // Get recording targets
        let targets = scap::get_all_targets();
        info!("Targets: {:?}", targets);

        // Create capturer options (following scap example pattern)
        let options = Options {
            fps: 30,
            target: None, // None captures the primary display
            show_cursor,
            show_highlight: false,
            excluded_targets: None,
            output_type: FrameType::BGRAFrame,
            output_resolution: Resolution::_720p,
            crop_area: None, // Capture full screen
            ..Default::default()
        };

        // Create capturer using the build() method (following scap example)
        let mut capturer = Capturer::build(options)
            .map_err(|e| format!("Problem with building Capturer: {}", e))?;
        
        // Start capture
        capturer.start_capture();
        
        // Use default dimensions (will be updated when first frame arrives)
        let (width, height) = (1920, 1080);
        
        info!("Initialized scap screen capture ({}x{}) with cursor: {}, format: {:?}", 
              width, height, show_cursor, output_format);
        
        // Define tile size (64x64 is a good balance)
        let tile_size = 64;
        
        // Calculate how many tiles we need
        let tiles_x = (width + tile_size - 1) / tile_size;
        let tiles_y = (height + tile_size - 1) / tile_size;
        let total_tiles = tiles_x * tiles_y;
        
        // Initialize empty tiles
        let tiles = vec![
            ScreenTile {
                data: Vec::new(),
                hash: 0,
                changed: false,
            };
            total_tiles
        ];
        
        // Initialize YUV converter if needed
        let yuv_converter = match output_format {
            OutputFormat::YUV420 => Some(YuvConverter::new(width, height)),
            _ => None,
        };
        
        Ok(Self {
            capturer: Some(capturer),
            width,
            height,
            tile_size,
            tiles,
            previous_frame: None,
            adaptive_quality: Mutex::new(80),
            monitor_id: format!("scap-target-{}", monitor_index.unwrap_or(0)),
            is_primary: monitor_index.is_none() || monitor_index == Some(0),
            show_cursor,
            output_format,
            capture_fps: 30,
            yuv_converter,
        })
    }

    // Get a list of all available monitors (simplified)
    pub fn get_all_monitors() -> Result<Vec<MonitorInfo>, Box<dyn std::error::Error>> {
        // Check if the platform is supported
        if !is_supported() {
            return Err("❌ Platform not supported".into());
        }

        // Check if we have permission to capture screen
        if !has_permission() {
            warn!("❌ Permission not granted for monitor enumeration");
        }

        // For simplicity, return a default primary monitor
        // scap will handle target selection internally when None is used
        let monitor_info = MonitorInfo {
            id: "primary".to_string(),
            name: "Primary Display".to_string(),
            is_primary: true,
            width: 1920, // Default values - actual values determined at capture time
            height: 1080,
            position_x: 0,
            position_y: 0,
            scale_factor: 1.0,
            rotation: 0,
            supports_cursor: true,
            supports_highlight: true,
        };
        
        Ok(vec![monitor_info])
    }

    // Enhanced capture_raw method following scap example pattern
    pub fn capture_raw(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Check if we have a capturer
        let capturer = self.capturer.as_mut()
            .ok_or("No capturer available")?;
        
        // Get the next frame (following scap example pattern)
        let frame = capturer.get_next_frame()
            .map_err(|e| format!("Error getting frame: {}", e))?;
        
        // Update dimensions from frame and convert to RGBA format
        let rgba_buffer = match frame {
            Frame::BGRA(f) => {
                // Update dimensions
                self.width = f.width as usize;
                self.height = f.height as usize;
                
                // Convert BGRA to RGBA
                let mut rgba = Vec::with_capacity(f.data.len());
                for chunk in f.data.chunks_exact(4) {
                    rgba.push(chunk[2]); // R (from B)
                    rgba.push(chunk[1]); // G
                    rgba.push(chunk[0]); // B (from R)
                    rgba.push(chunk[3]); // A
                }
                rgba
            },
            Frame::RGB(f) => {
                // Update dimensions
                self.width = f.width as usize;
                self.height = f.height as usize;
                
                // Convert RGB to RGBA
                let mut rgba = Vec::with_capacity(f.data.len() * 4 / 3);
                for chunk in f.data.chunks_exact(3) {
                    rgba.push(chunk[0]); // R
                    rgba.push(chunk[1]); // G
                    rgba.push(chunk[2]); // B
                    rgba.push(255);      // A
                }
                rgba
            },
            Frame::RGBx(f) => {
                // Update dimensions
                self.width = f.width as usize;
                self.height = f.height as usize;
                
                // Convert RGBx to RGBA
                let mut rgba = Vec::with_capacity(f.data.len());
                for chunk in f.data.chunks_exact(4) {
                    rgba.push(chunk[0]); // R
                    rgba.push(chunk[1]); // G
                    rgba.push(chunk[2]); // B
                    rgba.push(255);      // A (ignore X)
                }
                rgba
            },
            Frame::BGRx(f) => {
                // Update dimensions
                self.width = f.width as usize;
                self.height = f.height as usize;
                
                // Convert BGRx to RGBA
                let mut rgba = Vec::with_capacity(f.data.len());
                for chunk in f.data.chunks_exact(4) {
                    rgba.push(chunk[2]); // R (from B)
                    rgba.push(chunk[1]); // G
                    rgba.push(chunk[0]); // B (from R)
                    rgba.push(255);      // A (ignore X)
                }
                rgba
            },
            Frame::XBGR(f) => {
                // Update dimensions
                self.width = f.width as usize;
                self.height = f.height as usize;
                
                // Convert XBGR to RGBA
                let mut rgba = Vec::with_capacity(f.data.len());
                for chunk in f.data.chunks_exact(4) {
                    rgba.push(chunk[3]); // R (from last)
                    rgba.push(chunk[2]); // G 
                    rgba.push(chunk[1]); // B 
                    rgba.push(255);      // A (ignore X)
                }
                rgba
            },
            Frame::BGR0(f) => {
                // Update dimensions
                self.width = f.width as usize;
                self.height = f.height as usize;
                
                // Convert BGR0 to RGBA
                let mut rgba = Vec::with_capacity(f.data.len());
                for chunk in f.data.chunks_exact(4) {
                    rgba.push(chunk[2]); // R (from B)
                    rgba.push(chunk[1]); // G
                    rgba.push(chunk[0]); // B (from R)
                    rgba.push(255);      // A (ignore 0)
                }
                rgba
            },
            Frame::YUVFrame(f) => {
                // Update dimensions
                self.width = f.width as usize;
                self.height = f.height as usize;
                
                // For YUV, we'd need proper conversion - for now return error
                return Err("YUV frame conversion not implemented yet".into());
            }
        };
        
        // Update YUV converter if dimensions changed
        if matches!(self.output_format, OutputFormat::YUV420) {
            self.yuv_converter = Some(YuvConverter::new(self.width, self.height));
        }
        
        // Apply format conversion if needed
        let output_buffer = match self.output_format {
            OutputFormat::RGBA => rgba_buffer.clone(),
            OutputFormat::BGRA => {
                // Convert RGBA to BGRA
                let mut bgra_buffer = Vec::with_capacity(rgba_buffer.len());
                for chunk in rgba_buffer.chunks_exact(4) {
                    bgra_buffer.push(chunk[2]); // B
                    bgra_buffer.push(chunk[1]); // G
                    bgra_buffer.push(chunk[0]); // R
                    bgra_buffer.push(chunk[3]); // A
                }
                bgra_buffer
            },
            OutputFormat::YUV420 => {
                // Convert RGBA to YUV420
                if let Some(ref converter) = self.yuv_converter {
                    converter.rgba_to_yuv420(&rgba_buffer)
                } else {
                    return Err("YUV converter not initialized".into());
                }
            }
        };
        
        // Store as previous frame for delta encoding
        self.previous_frame = Some(output_buffer.clone());
        
        Ok(output_buffer)
    }

    pub fn capture_rgba(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // If current format is already RGBA, use direct capture
        if matches!(self.output_format, OutputFormat::RGBA) {
            return self.capture_raw();
        }
        
        // Temporarily switch to RGBA format
        let original_format = self.output_format.clone();
        self.output_format = OutputFormat::RGBA;
        
        // Update YUV converter if needed
        if matches!(original_format, OutputFormat::YUV420) {
            self.yuv_converter = None; // Disable for RGBA
        }
        
        let result = self.capture_raw();
        
        // Restore original format
        self.output_format = original_format;
        if matches!(self.output_format, OutputFormat::YUV420) {
            self.yuv_converter = Some(YuvConverter::new(self.width, self.height));
        }
        
        result
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn tile_dimensions(&self) -> (usize, usize, usize) {
        let tiles_x = (self.width + self.tile_size - 1) / self.tile_size;
        let tiles_y = (self.height + self.tile_size - 1) / self.tile_size;
        (tiles_x, tiles_y, self.tile_size)
    }

    pub fn update_quality(&self, quality: u8) {
        let mut current_quality = self.adaptive_quality.lock().unwrap();
        *current_quality = quality.clamp(1, 100);
    }
    
    pub fn get_monitor_id(&self) -> &str {
        &self.monitor_id
    }
    
    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    // Enhanced methods (preparing for scap migration)
    
    // Enable/disable cursor capture (native scap support)
    pub fn set_cursor_capture(&mut self, show_cursor: bool) -> Result<(), Box<dyn std::error::Error>> {
        self.show_cursor = show_cursor;
        // Note: To actually change cursor capture, we'd need to recreate the capturer
        // For now, just update the preference and log
        info!("Cursor capture preference set to: {} (scap native support available)", show_cursor);
        Ok(())
    }
    
    // Set capture FPS preference (for future scap compatibility)
    pub fn set_fps(&mut self, fps: u32) -> Result<(), Box<dyn std::error::Error>> {
        self.capture_fps = fps;
        info!("Capture FPS preference set to: {}", fps);
        Ok(())
    }
    
    // Set output format
    pub fn set_output_format(&mut self, format: OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
        self.output_format = format.clone();
        
        // Update YUV converter based on format
        match format {
            OutputFormat::YUV420 => {
                self.yuv_converter = Some(YuvConverter::new(self.width, self.height));
            },
            _ => {
                self.yuv_converter = None;
            }
        }
        
        info!("Output format set to: {:?}", format);
        Ok(())
    }
    
    // Check if cursor capture is enabled
    pub fn cursor_enabled(&self) -> bool {
        self.show_cursor
    }
    
    // Get current FPS setting
    pub fn get_fps(&self) -> u32 {
        self.capture_fps
    }
    
    // Get current output format
    pub fn get_output_format(&self) -> &OutputFormat {
        &self.output_format
    }
    
    // Capture YUV frame directly (for streaming efficiency)
    pub fn capture_yuv(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Temporarily switch to YUV format if not already
        if !matches!(self.output_format, OutputFormat::YUV420) {
            let original_format = self.output_format.clone();
            self.output_format = OutputFormat::YUV420;
            self.yuv_converter = Some(YuvConverter::new(self.width, self.height));
            
            let result = self.capture_raw();
            
            // Restore original format
            match original_format {
                OutputFormat::YUV420 => {
                    self.output_format = original_format; // Keep YUV converter
                }, 
                _ => {
                    self.output_format = original_format;
                    self.yuv_converter = None;
                }
            }
            
            result
        } else {
            self.capture_raw()
        }
    }
    
    // Get supported capabilities
    pub fn get_capabilities(&self) -> Vec<String> {
        vec![
            "rgba_output".to_string(),
            "bgra_output".to_string(), 
            "yuv420_output".to_string(),
            "format_conversion".to_string(),
            "tile_based_capture".to_string(),
            "adaptive_quality".to_string(),
            "cursor_capture".to_string(), // Native scap support
            "highlight_support".to_string(), // Native scap support
            "high_fps_capture".to_string(), // scap optimization
        ]
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        if let Some(ref mut capturer) = self.capturer {
            capturer.stop_capture();
            info!("ScreenCapture stopped and cleaned up");
        }
    }
}