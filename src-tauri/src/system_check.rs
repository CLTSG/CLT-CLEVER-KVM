use log::{info, warn, debug};
use std::process::Command;

pub struct SystemCapabilities {
    pub has_nvidia_gpu: bool,
    pub has_nvidia_drivers: bool,
    pub has_cuda: bool,
    pub available_encoders: Vec<String>,
}

impl SystemCapabilities {
    pub fn check() -> Self {
        let mut caps = SystemCapabilities {
            has_nvidia_gpu: false,
            has_nvidia_drivers: false,
            has_cuda: false,
            available_encoders: Vec::new(),
        };
        
        // Check for NVIDIA GPU
        caps.has_nvidia_gpu = Self::check_nvidia_gpu();
        
        // Check for NVIDIA drivers
        caps.has_nvidia_drivers = Self::check_nvidia_drivers();
        
        // Check for CUDA
        caps.has_cuda = Self::check_cuda();
        
        // Check available FFmpeg encoders
        caps.available_encoders = Self::check_ffmpeg_encoders();
        
        info!("System capabilities: NVIDIA GPU: {}, NVIDIA drivers: {}, CUDA: {}", 
              caps.has_nvidia_gpu, caps.has_nvidia_drivers, caps.has_cuda);
        debug!("Available encoders: {:?}", caps.available_encoders);
        
        caps
    }
    
    fn check_nvidia_gpu() -> bool {
        // Check if lspci shows NVIDIA GPU
        match Command::new("lspci").output() {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                output_str.to_lowercase().contains("nvidia")
            },
            Err(_) => {
                // Try alternative method
                std::path::Path::new("/proc/driver/nvidia").exists()
            }
        }
    }
    
    fn check_nvidia_drivers() -> bool {
        // Check nvidia-smi
        match Command::new("nvidia-smi").arg("--query-gpu=name").arg("--format=csv,noheader").output() {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
    
    fn check_cuda() -> bool {
        // Check if CUDA library can be loaded
        std::path::Path::new("/usr/local/cuda/lib64/libcuda.so").exists() ||
        std::path::Path::new("/usr/lib/x86_64-linux-gnu/libcuda.so.1").exists() ||
        std::path::Path::new("/usr/lib64/libcuda.so.1").exists()
    }
    
    fn check_ffmpeg_encoders() -> Vec<String> {
        let mut encoders = Vec::new();
        
        // Initialize FFmpeg and check available encoders
        if let Ok(_) = ffmpeg_next::init() {
            let encoder_names = [
                "libx264", "h264_nvenc", "h264_vaapi", "h264_qsv",
                "libx265", "hevc_nvenc", "hevc_vaapi", "hevc_qsv",
                "libaom-av1", "av1_nvenc", "av1_vaapi", "av1_qsv",
                "mjpeg"
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
            "h264" => {
                if self.available_encoders.contains(&"h264_nvenc".to_string()) && self.has_cuda {
                    Some("h264_nvenc".to_string())
                } else if self.available_encoders.contains(&"h264_vaapi".to_string()) {
                    Some("h264_vaapi".to_string())
                } else if self.available_encoders.contains(&"libx264".to_string()) {
                    Some("libx264".to_string())
                } else {
                    None
                }
            },
            "h265" => {
                if self.available_encoders.contains(&"hevc_nvenc".to_string()) && self.has_cuda {
                    Some("hevc_nvenc".to_string())
                } else if self.available_encoders.contains(&"hevc_vaapi".to_string()) {
                    Some("hevc_vaapi".to_string())
                } else if self.available_encoders.contains(&"libx265".to_string()) {
                    Some("libx265".to_string())
                } else {
                    None
                }
            },
            "av1" => {
                if self.available_encoders.contains(&"av1_nvenc".to_string()) && self.has_cuda {
                    Some("av1_nvenc".to_string())
                } else if self.available_encoders.contains(&"libaom-av1".to_string()) {
                    Some("libaom-av1".to_string())
                } else {
                    None
                }
            },
            _ => None
        }
    }
}
