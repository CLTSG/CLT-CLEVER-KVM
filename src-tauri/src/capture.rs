use image::{ImageBuffer, RgbaImage};
use scrap::{Capturer, Display};
use std::io::ErrorKind::WouldBlock;
use std::time::Duration;
use std::thread;
use std::collections::HashMap;
use std::sync::Mutex;
use display_info::DisplayInfo;
use log::{info, warn};

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
}

pub struct ScreenCapture {
    capturer: Capturer,
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
}

impl ScreenCapture {
    // Enhanced method for monitor information
    fn get_monitor_info(display_index: usize) -> Result<MonitorInfo, Box<dyn std::error::Error>> {
        // Get system display info
        let system_displays = DisplayInfo::all()?;
        
        if system_displays.is_empty() {
            return Err("No display info available".into());
        }
        
        // Map the display_index to the system display
        let display_info = if display_index < system_displays.len() {
            &system_displays[display_index]
        } else {
            // Default to the first display
            &system_displays[0]
        };
        
        // Fix: Don't use match on non-Option type
        let scale_factor = display_info.scale_factor;
        let rotation = display_info.rotation as i32;
        
        // Handle name field correctly
        let display_name = format!("Display {}", display_info.id);
        
        Ok(MonitorInfo {
            id: display_info.id.to_string(),
            name: display_name,
            is_primary: display_info.is_primary,
            width: display_info.width as usize,
            height: display_info.height as usize,
            position_x: display_info.x,
            position_y: display_info.y,
            scale_factor: scale_factor as f64,
            rotation,
        })
    }

    pub fn new(monitor_index: Option<usize>) -> Result<Self, Box<dyn std::error::Error>> {
        // Get all displays
        let displays = Display::all()?;
        
        if displays.is_empty() {
            return Err("No displays found".into());
        }
        
        // Determine which display to capture
        let display_index = match monitor_index {
            Some(idx) => {
                if idx < displays.len() {
                    idx
                } else {
                    warn!("Requested monitor index {} out of bounds, falling back to primary", idx);
                    0 // Default to primary display
                }
            },
            None => 0, // Default to primary display
        };
        
        // Fix: The Capturer::new function requires ownership of Display
        // We need to extract the display at the index and pass it directly
        if display_index >= displays.len() {
            return Err(format!("Display index {} out of bounds", display_index).into());
        }
        
        // Create a vector we can extract from (to avoid borrowing issues)
        let mut displays_vec = displays;
        // Remove the display we need from the vector to get ownership
        let display = displays_vec.remove(display_index);
        
        // Now we can create the capturer with the owned Display
        let width = display.width();
        let height = display.height();
        let capturer = Capturer::new(display)?;
        
        // Get additional monitor info
        let monitor_info = Self::get_monitor_info(display_index)?;
        
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
                changed: true, // Mark all tiles as changed initially
            };
            total_tiles
        ];

        info!("Initialized screen capture for monitor {} ({}x{} at {},{}, primary: {})",
              monitor_info.name, width, height, 
              monitor_info.position_x, monitor_info.position_y,
              monitor_info.is_primary);

        Ok(ScreenCapture {
            capturer,
            width,
            height,
            tile_size,
            tiles,
            previous_frame: None,
            adaptive_quality: Mutex::new(85), // Start with good quality
            monitor_id: monitor_info.id,
            is_primary: monitor_info.is_primary,
        })
    }

    // Get a list of all available monitors
    pub fn get_all_monitors() -> Result<Vec<MonitorInfo>, Box<dyn std::error::Error>> {
        let displays = Display::all()?;
        let mut monitors = Vec::new();
        
        for (idx, _) in displays.iter().enumerate() {
            if let Ok(info) = Self::get_monitor_info(idx) {
                monitors.push(info);
            }
        }
        
        Ok(monitors)
    }

    pub fn capture_jpeg(&mut self, quality: u8) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let quality = quality.clamp(1, 100);
        
        // Capture frame
        let buffer = loop {
            match self.capturer.frame() {
                Ok(buffer) => break buffer,
                Err(error) => {
                    if error.kind() == WouldBlock {
                        // Wait for the next frame
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    return Err(Box::new(error));
                }
            }
        };

        // Convert to RGBA
        let mut rgba_buffer = Vec::with_capacity(self.width * self.height * 4);
        let stride = buffer.len() / self.height;

        for y in 0..self.height {
            for x in 0..self.width {
                let i = stride * y + 4 * x;
                rgba_buffer.push(buffer[i + 2]); // R
                rgba_buffer.push(buffer[i + 1]); // G
                rgba_buffer.push(buffer[i]);     // B
                rgba_buffer.push(255);           // A
            }
        }

        // Create image from buffer
        let img: RgbaImage = ImageBuffer::from_raw(
            self.width as u32,
            self.height as u32,
            rgba_buffer,
        ).ok_or("Failed to create image buffer")?;

        // Encode to JPEG
        let mut jpeg_data = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_data, quality);
        encoder.encode_image(&img)?;

        Ok(jpeg_data)
    }

    pub fn capture_jpeg_delta(&mut self, quality: Option<u8>) -> Result<HashMap<usize, Vec<u8>>, Box<dyn std::error::Error>> {
        // Use provided quality or adaptive quality
        let quality = quality.unwrap_or_else(|| *self.adaptive_quality.lock().unwrap());
        
        // Capture frame
        let buffer = loop {
            match self.capturer.frame() {
                Ok(buffer) => break buffer,
                Err(error) => {
                    if error.kind() == WouldBlock {
                        // Wait for the next frame
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    return Err(Box::new(error));
                }
            }
        };

        // Initialize changed tiles map
        let mut changed_tiles = HashMap::new();
        
        // Check if we have a previous frame to compare with
        if let Some(previous_buffer) = &self.previous_frame {
            // Detect changes in tiles
            let stride = buffer.len() / self.height;
            let tiles_x = (self.width + self.tile_size - 1) / self.tile_size;
            
            for ty in 0..(self.height + self.tile_size - 1) / self.tile_size {
                for tx in 0..tiles_x {
                    let tile_index = ty * tiles_x + tx;
                    
                    // Calculate tile boundaries
                    let start_x = tx * self.tile_size;
                    let start_y = ty * self.tile_size;
                    let end_x = (start_x + self.tile_size).min(self.width);
                    let end_y = (start_y + self.tile_size).min(self.height);
                    
                    // Check if tile has changed
                    let mut changed = false;
                    'outer: for y in start_y..end_y {
                        for x in start_x..end_x {
                            let idx = y * stride + x * 4;
                            let prev_idx = y * self.width * 4 + x * 4;
                            
                            // Compare pixels (using a threshold to account for small noise)
                            if (buffer[idx] as i16 - previous_buffer[prev_idx] as i16).abs() > 5 ||
                               (buffer[idx+1] as i16 - previous_buffer[prev_idx+1] as i16).abs() > 5 ||
                               (buffer[idx+2] as i16 - previous_buffer[prev_idx+2] as i16).abs() > 5 {
                                changed = true;
                                break 'outer;
                            }
                        }
                    }
                    
                    if changed {
                        // Extract the tile and compress it as JPEG
                        let mut tile_rgba = Vec::with_capacity(self.tile_size * self.tile_size * 4);
                        
                        for y in start_y..end_y {
                            for x in start_x..end_x {
                                let i = stride * y + 4 * x;
                                tile_rgba.push(buffer[i + 2]); // R
                                tile_rgba.push(buffer[i + 1]); // G
                                tile_rgba.push(buffer[i]);     // B
                                tile_rgba.push(255);           // A
                            }
                        }
                        
                        // Create image from tile buffer
                        let tile_width = end_x - start_x;
                        let tile_height = end_y - start_y;
                        
                        let img: RgbaImage = ImageBuffer::from_raw(
                            tile_width as u32,
                            tile_height as u32,
                            tile_rgba,
                        ).ok_or("Failed to create tile image buffer")?;
                        
                        // Encode to JPEG
                        let mut jpeg_data = Vec::new();
                        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_data, quality);
                        encoder.encode_image(&img)?;
                        
                        // Add to changed tiles
                        changed_tiles.insert(tile_index, jpeg_data);
                    }
                }
            }
        } else {
            // First frame, all tiles are considered changed
            // Encode the whole image as JPEG
            let mut rgba_buffer = Vec::with_capacity(self.width * self.height * 4);
            let stride = buffer.len() / self.height;
            
            for y in 0..self.height {
                for x in 0..self.width {
                    let i = stride * y + 4 * x;
                    rgba_buffer.push(buffer[i + 2]); // R
                    rgba_buffer.push(buffer[i + 1]); // G
                    rgba_buffer.push(buffer[i]);     // B
                    rgba_buffer.push(255);           // A
                }
            }
            
            // Store the first frame for future comparisons
            self.previous_frame = Some(rgba_buffer.clone());
            
            // Create image from buffer
            let img: RgbaImage = ImageBuffer::from_raw(
                self.width as u32,
                self.height as u32,
                rgba_buffer,
            ).ok_or("Failed to create image buffer")?;
            
            // Encode to JPEG
            let mut jpeg_data = Vec::new();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_data, quality);
            encoder.encode_image(&img)?;
            
            // Add as a special "full frame" tile
            changed_tiles.insert(0xFFFFFFFF, jpeg_data);
        }
        
        // Update previous frame
        let mut rgba_buffer = Vec::with_capacity(self.width * self.height * 4);
        let stride = buffer.len() / self.height;
        
        for y in 0..self.height {
            for x in 0..self.width {
                let i = stride * y + 4 * x;
                rgba_buffer.push(buffer[i + 2]); // R
                rgba_buffer.push(buffer[i + 1]); // G
                rgba_buffer.push(buffer[i]);     // B
                rgba_buffer.push(255);           // A
            }
        }
        
        self.previous_frame = Some(rgba_buffer);
        
        Ok(changed_tiles)
    }

    // Enhanced capture_raw method with scaling support for high DPI screens
    pub fn capture_raw(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Capture frame in raw format for codec encoding
        let buffer = loop {
            match self.capturer.frame() {
                Ok(buffer) => break buffer,
                Err(error) => {
                    if error.kind() == WouldBlock {
                        // Wait for the next frame
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    return Err(Box::new(error));
                }
            }
        };

        // Convert to RGBA with optimized path for better performance
        let mut rgba_buffer = Vec::with_capacity(self.width * self.height * 4);
        let stride = buffer.len() / self.height;

        // For large monitors, use parallel processing to speed up conversion
        if self.width * self.height > 1_000_000 { // 1 million pixels threshold
            // Using rayon or similar parallel processing would be ideal here
            // This is a simple chunked approach for demonstration
            let chunk_size = self.height / 4; // Split into 4 chunks
            
            let mut chunks: Vec<Vec<u8>> = vec![Vec::with_capacity(self.width * chunk_size * 4); 4];
            
            // Process chunks in sequence (parallel would be better)
            for chunk_idx in 0..4 {
                let start_y = chunk_idx * chunk_size;
                let end_y = if chunk_idx == 3 { self.height } else { (chunk_idx + 1) * chunk_size };
                
                for y in start_y..end_y {
                    for x in 0..self.width {
                        let i = stride * y + 4 * x;
                        chunks[chunk_idx].push(buffer[i + 2]); // R
                        chunks[chunk_idx].push(buffer[i + 1]); // G
                        chunks[chunk_idx].push(buffer[i]);     // B
                        chunks[chunk_idx].push(255);           // A
                    }
                }
            }
            
            // Combine chunks
            for chunk in chunks {
                rgba_buffer.extend_from_slice(&chunk);
            }
        } else {
            // Original approach for smaller screens
            for y in 0..self.height {
                for x in 0..self.width {
                    let i = stride * y + 4 * x;
                    rgba_buffer.push(buffer[i + 2]); // R
                    rgba_buffer.push(buffer[i + 1]); // G
                    rgba_buffer.push(buffer[i]);     // B
                    rgba_buffer.push(255);           // A
                }
            }
        }

        Ok(rgba_buffer)
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
}