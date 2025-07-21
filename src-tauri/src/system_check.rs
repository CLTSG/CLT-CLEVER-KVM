use log::{info, debug};
use std::process::Command;

pub struct SystemCapabilities {
    pub available_encoders: Vec<String>,
}

impl SystemCapabilities {
    pub fn check() -> Self {
        let mut caps = SystemCapabilities {
            available_encoders: Vec::new(),
        };
        
        // Check available FFmpeg encoders
        caps.available_encoders = Self::check_ffmpeg_encoders();
        
        info!("System capabilities checked");
        debug!("Available encoders: {:?}", caps.available_encoders);
        
        caps
    }
    
    fn check_ffmpeg_encoders() -> Vec<String> {
        let mut encoders = Vec::new();
        
        // Initialize FFmpeg and check available encoders
        if let Ok(_) = ffmpeg_next::init() {
            let encoder_names = [
                "libvpx", "libvpx-vp8"
            ];
            
            for name in &encoder_names {
                if ffmpeg_next::encoder::find_by_name(name).is_some() {
                    encoders.push(name.to_string());
                }
            }
        }
        
        encoders
    }
    
    pub fn get_recommended_encoder(&self, codec_type: &str) -> Option<String> {
        match codec_type {
            "vp8" => {
                if self.available_encoders.contains(&"libvpx".to_string()) {
                    Some("libvpx".to_string())
                } else if self.available_encoders.contains(&"libvpx-vp8".to_string()) {
                    Some("libvpx-vp8".to_string())
                } else {
                    None
                }
            },
            _ => None
        }
    }
}
