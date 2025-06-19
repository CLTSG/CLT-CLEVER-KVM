use image::{ImageBuffer, RgbaImage};
use scrap::{Capturer, Display};
use std::io::ErrorKind::WouldBlock;
use std::time::Duration;
use std::thread;
use std::collections::HashMap;
use std::sync::Mutex;

// For delta encoding
#[derive(Clone)]
pub struct ScreenTile {
    pub data: Vec<u8>,
    pub hash: u64,
    pub changed: bool,
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
}

impl ScreenCapture {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let display = Display::primary()?;
        let width = display.width();
        let height = display.height();
        let capturer = Capturer::new(display)?;
        
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

        Ok(ScreenCapture {
            capturer,
            width,
            height,
            tile_size,
            tiles,
            previous_frame: None,
            adaptive_quality: Mutex::new(85), // Start with good quality
        })
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
}
