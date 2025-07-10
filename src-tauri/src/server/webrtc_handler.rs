use crate::codec::{WebRTCVideoEncoder, WebRTCEncoderConfig};
use crate::capture::ScreenCapture;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::broadcast;
use log::{error, info, warn, debug};
use serde::{Deserialize, Serialize};
use crate::server::models::NetworkStats;

// Quality profiles for adaptive streaming
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QualityProfile {
    High,
    Medium,
    Low,
}

impl QualityProfile {
    pub fn get_config(&self, width: u32, height: u32) -> WebRTCEncoderConfig {
        match self {
            QualityProfile::High => WebRTCEncoderConfig {
                width,
                height,
                bitrate: 8000000, // 8 Mbps
                framerate: 60,
                keyframe_interval: 60,
                quality_preset: "fast".to_string(),
                use_hardware: true,
                ..Default::default()
            },
            QualityProfile::Medium => WebRTCEncoderConfig {
                width: (width * 3 / 4).max(720), // 75% or min 720p
                height: (height * 3 / 4).max(480),
                bitrate: 4000000, // 4 Mbps
                framerate: 30,
                keyframe_interval: 30,
                quality_preset: "medium".to_string(),
                use_hardware: true,
                ..Default::default()
            },
            QualityProfile::Low => WebRTCEncoderConfig {
                width: (width / 2).max(640), // 50% or min 640p
                height: (height / 2).max(360),
                bitrate: 1500000, // 1.5 Mbps
                framerate: 24,
                keyframe_interval: 24,
                quality_preset: "veryfast".to_string(),
                use_hardware: false,
                ..Default::default()
            },
        }
    }
    
    pub fn from_network_stats(stats: &NetworkStats) -> Self {
        // Auto-select quality based on network conditions
        let bandwidth_mbps = stats.bandwidth;
        let latency_ms = stats.latency;
        let packet_loss = stats.packet_loss;
        
        // High quality: Good bandwidth (>6 Mbps), low latency (<50ms), minimal packet loss (<1%)
        if bandwidth_mbps > 6.0 && latency_ms < 50 && packet_loss < 1.0 {
            QualityProfile::High
        }
        // Medium quality: Decent bandwidth (>3 Mbps), moderate latency (<150ms), some packet loss (<3%)
        else if bandwidth_mbps > 3.0 && latency_ms < 150 && packet_loss < 3.0 {
            QualityProfile::Medium
        }
        // Low quality: For everything else
        else {
            QualityProfile::Low
        }
    }
}

pub struct WebRTCStreamingSession {
    encoder: Arc<Mutex<WebRTCVideoEncoder>>,
    capture: Arc<Mutex<ScreenCapture>>,
    stats: Arc<Mutex<StreamingStats>>,
    control_rx: mpsc::Receiver<StreamingControl>,
    frame_tx: broadcast::Sender<EncodedFrameMessage>,
    current_quality: Arc<Mutex<QualityProfile>>,
    adaptive_controller: Arc<Mutex<AdaptiveBitrateController>>,
}

#[derive(Debug, Clone)]
pub struct StreamingStats {
    pub frames_encoded: u64,
    pub bytes_sent: u64,
    pub keyframes_sent: u32,
    pub encoding_time_ms: u64,
    pub capture_time_ms: u64,
    pub last_bitrate_kbps: u32,
    pub dropped_frames: u32,
    pub current_fps: f32,
    pub target_fps: u32,
    pub network_quality_score: f32, // 0.0 to 1.0
    pub last_quality_change: Instant,
}

#[derive(Debug)]
pub enum StreamingControl {
    UpdateBitrate(u32),
    RequestKeyframe,
    UpdateFramerate(u32),
    UpdateQuality(QualityProfile),
    NetworkStatsUpdate(NetworkStats),
    Pause,
    Resume,
    Stop,
}

#[derive(Debug, Clone)]
pub struct EncodedFrameMessage {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub timestamp: u64,
    pub sequence_number: u32,
    pub rtp_packets: Vec<Vec<u8>>,
}

impl WebRTCStreamingSession {
    pub fn new(
        monitor_index: usize,
        initial_quality: QualityProfile,
    ) -> Result<(Self, mpsc::Sender<StreamingControl>, broadcast::Receiver<EncodedFrameMessage>), String> {
        // Get screen dimensions for quality configuration
        let capture = ScreenCapture::new(Some(monitor_index))
            .map_err(|e| format!("Failed to initialize screen capture: {}", e))?;
        
        let (width, height) = (capture.width() as u32, capture.height() as u32);
        let config = initial_quality.get_config(width, height);
        
        // Initialize encoder
        let encoder = Arc::new(Mutex::new(
            WebRTCVideoEncoder::new(config.clone())
                .map_err(|e| format!("Failed to initialize encoder: {}", e))?
        ));
        
        // Create control channels
        let (control_tx, control_rx) = mpsc::channel();
        let (frame_tx, frame_rx) = broadcast::channel(100);
        
        // Initialize stats
        let stats = Arc::new(Mutex::new(StreamingStats {
            frames_encoded: 0,
            bytes_sent: 0,
            keyframes_sent: 0,
            encoding_time_ms: 0,
            capture_time_ms: 0,
            last_bitrate_kbps: config.bitrate / 1000,
            dropped_frames: 0,
            current_fps: 0.0,
            target_fps: config.framerate,
            network_quality_score: 1.0,
            last_quality_change: Instant::now(),
        }));

        let capture_arc = Arc::new(Mutex::new(capture));
        let current_quality = Arc::new(Mutex::new(initial_quality));
        let adaptive_controller = Arc::new(Mutex::new(AdaptiveBitrateController::new()));

        let session = Self {
            encoder,
            capture: capture_arc,
            stats,
            control_rx,
            frame_tx,
            current_quality,
            adaptive_controller,
        };

        Ok((session, control_tx, frame_rx))
    }

    pub async fn run_streaming_loop(mut self) {
        self.streaming_loop().await;
    }

    async fn streaming_loop(mut self) {
        info!("Starting WebRTC H.264 streaming loop");
        
        let mut frame_count = 0u32;
        let mut last_keyframe = 0u32;
        let mut rtp_sequence = 1u16;
        let mut rtp_timestamp = 0u32;
        let ssrc = 0x12345678u32;
        
        // FPS tracking
        let mut fps_counter = 0u32;
        let mut last_fps_time = Instant::now();
        
        // Quality adaptation timing
        let mut last_quality_check = Instant::now();
        let quality_check_interval = Duration::from_secs(5);
        
        // Get target frame time from current quality
        let target_fps = {
            let quality = self.current_quality.lock().unwrap();
            let config = quality.get_config(1920, 1080); // Temp values
            config.framerate
        };
        let target_frame_time = Duration::from_millis(1000 / target_fps as u64);

        loop {
            let loop_start = Instant::now();
            
            // Check for control messages
            if let Ok(control_msg) = self.control_rx.try_recv() {
                match control_msg {
                    StreamingControl::Stop => {
                        info!("Received stop signal");
                        break;
                    },
                    StreamingControl::RequestKeyframe => {
                        info!("Keyframe requested");
                        last_keyframe = frame_count.saturating_sub(1000);
                    },
                    StreamingControl::UpdateQuality(new_quality) => {
                        self.update_quality(new_quality).await;
                    },
                    StreamingControl::NetworkStatsUpdate(stats) => {
                        self.handle_network_stats(stats).await;
                    },
                    StreamingControl::UpdateBitrate(new_bitrate) => {
                        self.update_bitrate(new_bitrate).await;
                    },
                    _ => {}
                }
            }

            // Periodic quality adaptation based on network conditions
            if last_quality_check.elapsed() >= quality_check_interval {
                self.adapt_quality_if_needed().await;
                last_quality_check = Instant::now();
            }

            // Capture and encode frame
            if let Ok(encoded_frame) = self.capture_and_encode_frame(frame_count, &mut last_keyframe).await {
                // Create RTP packets
                let rtp_packets = self.create_rtp_packets(
                    &encoded_frame.data,
                    &mut rtp_sequence,
                    &mut rtp_timestamp,
                    ssrc,
                    encoded_frame.is_keyframe,
                );

                // Send frame message
                let frame_message = EncodedFrameMessage {
                    data: encoded_frame.data,
                    is_keyframe: encoded_frame.is_keyframe,
                    timestamp: encoded_frame.timestamp,
                    sequence_number: frame_count,
                    rtp_packets,
                };

                if let Err(e) = self.frame_tx.send(frame_message) {
                    debug!("No receivers for frame data: {}", e);
                }
            }

            // Update FPS tracking
            fps_counter += 1;
            if last_fps_time.elapsed() >= Duration::from_secs(1) {
                let current_fps = fps_counter as f32 / last_fps_time.elapsed().as_secs_f32();
                if let Ok(mut stats) = self.stats.lock() {
                    stats.current_fps = current_fps;
                }
                fps_counter = 0;
                last_fps_time = Instant::now();
            }

            frame_count += 1;

            // Frame rate control
            let elapsed = loop_start.elapsed();
            if elapsed < target_frame_time {
                tokio::time::sleep(target_frame_time - elapsed).await;
            }
        }
        
        info!("WebRTC streaming loop ended");
    }

    async fn capture_and_encode_frame(&self, frame_count: u32, last_keyframe: &mut u32) -> Result<EncodedFrameData, String> {
        let capture_start = Instant::now();
        
        // Capture screen
        let rgba_data = {
            let mut capture = self.capture.lock().map_err(|e| format!("Failed to lock capture: {}", e))?;
            capture.capture_rgba().map_err(|e| format!("Failed to capture screen: {}", e))?
        };
        
        let capture_time = capture_start.elapsed();
        
        // Determine if keyframe is needed
        let is_keyframe = frame_count == 0 || (frame_count - *last_keyframe) >= 30;
        if is_keyframe {
            *last_keyframe = frame_count;
        }
        
        // Encode frame
        let encode_start = Instant::now();
        let encoded_data = {
            let mut encoder = self.encoder.lock().map_err(|e| format!("Failed to lock encoder: {}", e))?;
            encoder.encode_frame(&rgba_data, is_keyframe).map_err(|e| format!("Failed to encode frame: {}", e))?
        };
        let encode_time = encode_start.elapsed();
        
        // Update stats
        if let Ok(mut stats) = self.stats.lock() {
            stats.frames_encoded += 1;
            stats.bytes_sent += encoded_data.len() as u64;
            stats.capture_time_ms += capture_time.as_millis() as u64;
            stats.encoding_time_ms += encode_time.as_millis() as u64;
            if is_keyframe {
                stats.keyframes_sent += 1;
            }
        }
        
        Ok(EncodedFrameData {
            data: encoded_data,
            is_keyframe,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
    }

    async fn update_quality(&self, new_quality: QualityProfile) {
        if let Ok(mut current_quality) = self.current_quality.lock() {
            if *current_quality != new_quality {
                info!("Updating quality from {:?} to {:?}", *current_quality, new_quality);
                *current_quality = new_quality;
                
                // Update encoder configuration
                let (width, height) = {
                    let capture = self.capture.lock().unwrap();
                    (capture.width() as u32, capture.height() as u32)
                };
                
                let config = new_quality.get_config(width, height);
                
                // Reinitialize encoder with new config
                if let Ok(mut encoder) = self.encoder.lock() {
                    if let Ok(new_encoder) = WebRTCVideoEncoder::new(config.clone()) {
                        *encoder = new_encoder;
                        info!("Encoder updated for quality {:?}", new_quality);
                    }
                }
                
                // Update stats
                if let Ok(mut stats) = self.stats.lock() {
                    stats.target_fps = config.framerate;
                    stats.last_bitrate_kbps = config.bitrate / 1000;
                    stats.last_quality_change = Instant::now();
                }
            }
        }
    }

    async fn handle_network_stats(&self, network_stats: NetworkStats) {
        // Update adaptive controller
        if let Ok(mut controller) = self.adaptive_controller.lock() {
            controller.update_network_stats(network_stats.clone());
        }
        
        // Update quality score
        if let Ok(mut stats) = self.stats.lock() {
            stats.network_quality_score = Self::calculate_quality_score(&network_stats);
        }
        
        // Consider quality adaptation
        let recommended_quality = QualityProfile::from_network_stats(&network_stats);
        if let Ok(current_quality) = self.current_quality.lock() {
            if *current_quality != recommended_quality {
                // Don't change quality too frequently
                if let Ok(stats) = self.stats.lock() {
                    if stats.last_quality_change.elapsed() > Duration::from_secs(10) {
                        drop(stats);
                        drop(current_quality);
                        self.update_quality(recommended_quality).await;
                    }
                }
            }
        }
    }

    async fn adapt_quality_if_needed(&self) {
        if let Ok(mut controller) = self.adaptive_controller.lock() {
            if let Some(recommendation) = controller.get_quality_recommendation() {
                if let Ok(current_quality) = self.current_quality.lock() {
                    if *current_quality != recommendation {
                        drop(current_quality);
                        drop(controller);
                        self.update_quality(recommendation).await;
                    }
                }
            }
        }
    }

    async fn update_bitrate(&self, new_bitrate: u32) {
        if let Ok(mut stats) = self.stats.lock() {
            stats.last_bitrate_kbps = new_bitrate / 1000;
        }
        // TODO: Update encoder bitrate dynamically when encoder supports it
    }

    fn create_rtp_packets(
        &self,
        data: &[u8],
        rtp_sequence: &mut u16,
        rtp_timestamp: &mut u32,
        ssrc: u32,
        is_keyframe: bool,
    ) -> Vec<Vec<u8>> {
        let mut rtp_packets = Vec::new();
        let packet_size = 1200; // MTU-friendly size
        
        for (i, chunk) in data.chunks(packet_size).enumerate() {
            let mut rtp_packet = Vec::with_capacity(12 + chunk.len());
            
            // RTP header (12 bytes)
            let marker_bit = if i == (data.len() + packet_size - 1) / packet_size - 1 { 0x80 } else { 0x00 };
            rtp_packet.extend_from_slice(&[
                0x80, // V=2, P=0, X=0, CC=0
                0x60 | marker_bit, // M=marker, PT=96 (H.264)
            ]);
            rtp_packet.extend_from_slice(&rtp_sequence.to_be_bytes());
            rtp_packet.extend_from_slice(&rtp_timestamp.to_be_bytes());
            rtp_packet.extend_from_slice(&ssrc.to_be_bytes());
            
            // H.264 payload with NAL unit header
            if is_keyframe && i == 0 {
                // Add SPS/PPS for keyframes (simplified)
                rtp_packet.push(0x00); // NAL unit type
                rtp_packet.push(0x00);
                rtp_packet.push(0x00);
                rtp_packet.push(0x01); // Start code
                rtp_packet.push(0x67); // SPS NAL unit type
            }
            
            rtp_packet.extend_from_slice(chunk);
            rtp_packets.push(rtp_packet);
            
            *rtp_sequence = rtp_sequence.wrapping_add(1);
        }
        
        // Update RTP timestamp (90kHz for video)
        *rtp_timestamp = rtp_timestamp.wrapping_add(3000); // Assuming 30fps
        
        rtp_packets
    }

    fn calculate_quality_score(stats: &NetworkStats) -> f32 {
        let bandwidth_score = (stats.bandwidth / 10.0).min(1.0); // Normalize to 10 Mbps
        let latency_score = (200.0 - stats.latency as f32).max(0.0) / 200.0; // Normalize to 200ms
        let loss_score = (5.0 - stats.packet_loss).max(0.0) / 5.0; // Normalize to 5% loss
        
        (bandwidth_score + latency_score + loss_score) / 3.0
    }
}

#[derive(Debug)]
struct EncodedFrameData {
    data: Vec<u8>,
    is_keyframe: bool,
    timestamp: u64,
}

// Adaptive bitrate controller
pub struct AdaptiveBitrateController {
    network_history: Vec<NetworkStats>,
    quality_history: Vec<QualityProfile>,
    last_adaptation: Instant,
    consecutive_drops: u32,
    consecutive_improvements: u32,
    min_adaptation_interval: Duration,
}

impl AdaptiveBitrateController {
    pub fn new() -> Self {
        Self {
            network_history: Vec::new(),
            quality_history: Vec::new(),
            last_adaptation: Instant::now(),
            consecutive_drops: 0,
            consecutive_improvements: 0,
            min_adaptation_interval: Duration::from_secs(5),
        }
    }

    pub fn update_network_stats(&mut self, stats: NetworkStats) {
        self.network_history.push(stats);
        
        // Keep only last 10 measurements
        if self.network_history.len() > 10 {
            self.network_history.remove(0);
        }
    }

    pub fn get_quality_recommendation(&mut self) -> Option<QualityProfile> {
        if self.last_adaptation.elapsed() < self.min_adaptation_interval {
            return None;
        }

        if self.network_history.len() < 3 {
            return None;
        }

        let recent_stats = &self.network_history[self.network_history.len() - 3..];
        let avg_bandwidth = recent_stats.iter().map(|s| s.bandwidth).sum::<f32>() / recent_stats.len() as f32;
        let avg_latency = recent_stats.iter().map(|s| s.latency).sum::<u32>() / recent_stats.len() as u32;
        let avg_loss = recent_stats.iter().map(|s| s.packet_loss).sum::<f32>() / recent_stats.len() as f32;

        let avg_stats = NetworkStats {
            bandwidth: avg_bandwidth,
            latency: avg_latency,
            packet_loss: avg_loss,
        };

        let recommended_quality = QualityProfile::from_network_stats(&avg_stats);
        
        // Check if we should change quality
        let current_quality = self.quality_history.last().copied().unwrap_or(QualityProfile::Medium);
        
        if recommended_quality != current_quality {
            // Track consecutive changes
            match (current_quality, recommended_quality) {
                (QualityProfile::High, QualityProfile::Medium) |
                (QualityProfile::Medium, QualityProfile::Low) |
                (QualityProfile::High, QualityProfile::Low) => {
                    self.consecutive_drops += 1;
                    self.consecutive_improvements = 0;
                },
                (QualityProfile::Low, QualityProfile::Medium) |
                (QualityProfile::Medium, QualityProfile::High) |
                (QualityProfile::Low, QualityProfile::High) => {
                    self.consecutive_improvements += 1;
                    self.consecutive_drops = 0;
                },
                _ => {}
            }
            
            // Only change if we have enough evidence
            let should_change = match recommended_quality {
                QualityProfile::Low => self.consecutive_drops >= 1, // Quick drop to low
                QualityProfile::Medium => true, // Medium is always acceptable
                QualityProfile::High => self.consecutive_improvements >= 2, // Careful upgrade to high
            };
            
            if should_change {
                self.quality_history.push(recommended_quality);
                self.last_adaptation = Instant::now();
                
                // Keep only last 5 quality changes
                if self.quality_history.len() > 5 {
                    self.quality_history.remove(0);
                }
                
                info!("Quality adaptation: {:?} -> {:?} (bandwidth: {:.1} Mbps, latency: {}ms, loss: {:.1}%)",
                      current_quality, recommended_quality, avg_bandwidth, avg_latency, avg_loss);
                
                return Some(recommended_quality);
            }
        }
        
        None
    }
}
