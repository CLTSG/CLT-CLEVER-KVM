use xcap::Monitor;
use std::hash::Hash;
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
}

pub struct ScreenCapture {
    monitor: Monitor,
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
    // Getter methods for private fields
    pub fn width(&self) -> usize {
        self.width
    }
    
    pub fn height(&self) -> usize {
        self.height
    }

    pub fn new(monitor_index: Option<usize>) -> Result<Self, Box<dyn std::error::Error>> {
        // Get all monitors
        let monitors = Monitor::all().map_err(|e| format!("Failed to get monitors: {:?}", e))?;
        
        if monitors.is_empty() {
            return Err("No monitors found".into());
        }
        
        // Determine which monitor to capture
        let monitor_index = match monitor_index {
            Some(idx) => {
                if idx < monitors.len() {
                    idx
                } else {
                    warn!("Requested monitor index {} out of bounds, falling back to primary", idx);
                    0 // Default to primary monitor
                }
            },
            None => 0, // Default to primary monitor
        };
        
        let monitor = monitors.into_iter().nth(monitor_index)
            .ok_or_else(|| format!("Monitor index {} not found", monitor_index))?;
        
        let width = monitor.width() as usize;
        let height = monitor.height() as usize;
        
        info!("Initialized screen capture for monitor {} ({}x{})", 
              monitor.name(), width, height);
        
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
            monitor,
            width,
            height,
            tile_size,
            tiles,
            previous_frame: None,
            adaptive_quality: Mutex::new(85), // Start with good quality
            monitor_id: monitor_index.to_string(),
            is_primary: monitor_index == 0,
        })
    }

    // Get a list of all available monitors
    pub fn get_all_monitors() -> Result<Vec<MonitorInfo>, Box<dyn std::error::Error>> {
        let monitors = Monitor::all().map_err(|e| format!("Failed to get monitors: {:?}", e))?;
        let mut monitor_infos = Vec::new();
        
        for (idx, monitor) in monitors.iter().enumerate() {
            let monitor_info = MonitorInfo {
                id: idx.to_string(),
                name: monitor.name().to_string(),
                is_primary: idx == 0, // Assume first monitor is primary
                width: monitor.width() as usize,
                height: monitor.height() as usize,
                position_x: monitor.x(),
                position_y: monitor.y(),
                scale_factor: 1.0, // xcap doesn't provide scale factor directly
                rotation: 0,       // xcap doesn't provide rotation directly
            };
            monitor_infos.push(monitor_info);
        }
        
        Ok(monitor_infos)
    }

    // Enhanced capture_raw method with scaling support for high DPI screens
    pub fn capture_raw(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Capture screen using xcap
        let image = self.monitor.capture_image()
            .map_err(|e| format!("Failed to capture screen: {:?}", e))?;
        
        // Convert to raw RGBA bytes
        let rgba_buffer = image.into_raw();
        
        // Store previous frame for delta encoding
        self.previous_frame = Some(rgba_buffer.clone());
        
        Ok(rgba_buffer)
    }

    pub fn capture_rgba(&mut self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // For xcap, capture_raw already returns RGBA
        self.capture_raw()
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