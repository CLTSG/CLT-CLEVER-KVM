use crate::codec::{WebRTCVideoEncoder, WebRTCEncoderConfig};
use crate::capture::ScreenCapture;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::broadcast;
use log::{error, info, warn};

pub struct WebRTCStreamingSession {
    encoder: Arc<Mutex<WebRTCVideoEncoder>>,
    capture: Arc<Mutex<ScreenCapture>>,
    stats: Arc<Mutex<StreamingStats>>,
    control_rx: mpsc::Receiver<StreamingControl>,
    frame_tx: broadcast::Sender<EncodedFrameMessage>,
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
}

#[derive(Debug)]
pub enum StreamingControl {
    UpdateBitrate(u32),
    RequestKeyframe,
    UpdateFramerate(u32),
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
        config: WebRTCEncoderConfig,
    ) -> Result<(Self, mpsc::Sender<StreamingControl>, broadcast::Receiver<EncodedFrameMessage>), String> {
        // Initialize encoder only (we'll create capture per frame to avoid Send issues)
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
        }));

        // Create a dummy capture field (we'll create actual captures as needed)
        let capture = Arc::new(Mutex::new(
            ScreenCapture::new(Some(monitor_index))
                .map_err(|e| format!("Failed to initialize screen capture: {}", e))?
        ));

        let session = Self {
            encoder,
            capture,
            stats,
            control_rx,
            frame_tx,
        };

        Ok((session, control_tx, frame_rx))
    }

    // NOTE: This method is deprecated in favor of direct streaming in websocket handler
    // to avoid Send trait issues with ScreenCapture
    pub fn start_streaming(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // Placeholder - actual streaming is done in websocket handler
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        })
    }

    async fn async_streaming_loop(self) {
        info!("Starting WebRTC streaming loop");
        
        let mut frame_count = 0u32;
        let mut last_keyframe = 0u32;
        let target_fps = 30u32;
        let target_frame_time = Duration::from_millis(1000 / target_fps as u64);
        
        // RTP parameters
        let mut rtp_sequence = 1u16;
        let mut rtp_timestamp = 0u32;
        let ssrc = 0x12345678u32; // Static SSRC for this session

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
                        // Force next frame to be keyframe
                        last_keyframe = frame_count.saturating_sub(1000); // Force keyframe
                    },
                    StreamingControl::UpdateBitrate(new_bitrate) => {
                        info!("Updating bitrate to {} kbps", new_bitrate / 1000);
                        // TODO: Update encoder bitrate when encoder methods are available
                    },
                    _ => {}
                }
            }

            // Capture screen frame using available methods
            let capture_start = Instant::now();
            let frame_data = match self.capture.lock() {
                Ok(mut capture) => {
                    // Use the available capture_jpeg method
                    match capture.capture_jpeg(85) {
                        Ok(data) => data,
                        Err(e) => {
                            error!("Failed to capture screen: {}", e);
                            tokio::time::sleep(Duration::from_millis(33)).await; // ~30fps fallback
                            continue;
                        }
                    }
                },
                Err(e) => {
                    error!("Failed to lock capture: {}", e);
                    tokio::time::sleep(Duration::from_millis(33)).await;
                    continue;
                }
            };
            let capture_time = capture_start.elapsed();

            // For now, we'll send JPEG data wrapped as H.264-like packets
            // In a real WebRTC implementation, this would be actual H.264 encoding
            let encode_start = Instant::now();
            
            // Determine if this should be a keyframe
            let is_keyframe = frame_count == 0 || (frame_count - last_keyframe) >= 30; // Keyframe every 30 frames
            
            // Create RTP packets from the frame data
            let mut rtp_packets = Vec::new();
            let packet_size = 1200; // MTU-friendly size
            
            for (i, chunk) in frame_data.chunks(packet_size).enumerate() {
                let mut rtp_packet = Vec::with_capacity(12 + chunk.len());
                
                // RTP header (12 bytes)
                let marker_bit = if i == (frame_data.len() + packet_size - 1) / packet_size - 1 { 0x80 } else { 0x00 };
                rtp_packet.extend_from_slice(&[
                    0x80, // V=2, P=0, X=0, CC=0
                    0x60 | marker_bit, // M=marker, PT=96 (dynamic payload type for H.264)
                ]);
                rtp_packet.extend_from_slice(&rtp_sequence.to_be_bytes());
                rtp_packet.extend_from_slice(&rtp_timestamp.to_be_bytes());
                rtp_packet.extend_from_slice(&ssrc.to_be_bytes());
                
                // Payload
                rtp_packet.extend_from_slice(chunk);
                rtp_packets.push(rtp_packet);
                
                rtp_sequence = rtp_sequence.wrapping_add(1);
            }
            
            // Update RTP timestamp (90kHz for video)
            rtp_timestamp = rtp_timestamp.wrapping_add(3000); // 30fps = 3000 ticks per frame
            
            let encode_time = encode_start.elapsed();

            // Create frame message for WebRTC streaming
            let frame_message = EncodedFrameMessage {
                data: frame_data.clone(),
                is_keyframe,
                timestamp: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                sequence_number: frame_count,
                rtp_packets,
            };

            // Send frame to all subscribers
            if let Err(e) = self.frame_tx.send(frame_message) {
                warn!("No receivers for frame data: {}", e);
            }

            // Update stats
            if let Ok(mut stats) = self.stats.lock() {
                stats.frames_encoded += 1;
                stats.bytes_sent += frame_data.len() as u64;
                stats.encoding_time_ms += encode_time.as_millis() as u64;
                stats.capture_time_ms += capture_time.as_millis() as u64;
                
                if is_keyframe {
                    stats.keyframes_sent += 1;
                    last_keyframe = frame_count;
                }
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
    
    pub fn get_stats(&self) -> Option<StreamingStats> {
        self.stats.lock().ok().map(|stats| stats.clone())
    }
}

// Adaptive bitrate controller
pub struct AdaptiveBitrateController {
    current_bitrate: u32,
    target_bitrate: u32,
    min_bitrate: u32,
    max_bitrate: u32,
    last_adjustment: Instant,
    adjustment_interval: Duration,
}

impl AdaptiveBitrateController {
    pub fn new(initial_bitrate: u32) -> Self {
        Self {
            current_bitrate: initial_bitrate,
            target_bitrate: initial_bitrate,
            min_bitrate: initial_bitrate / 4,
            max_bitrate: initial_bitrate * 2,
            last_adjustment: Instant::now(),
            adjustment_interval: Duration::from_secs(2),
        }
    }
    
    pub fn current_bitrate(&self) -> u32 {
        self.current_bitrate
    }
    
    pub fn update_network_conditions(
        &mut self,
        latency_ms: u32,
        packet_loss_percent: f32,
        bandwidth_kbps: f32,
    ) -> Option<u32> {
        if self.last_adjustment.elapsed() < self.adjustment_interval {
            return None;
        }
        
        let mut adjustment_factor = 1.0;
        
        // Adjust based on latency
        if latency_ms > 200 {
            adjustment_factor *= 0.8; // Reduce bitrate for high latency
        } else if latency_ms < 50 {
            adjustment_factor *= 1.1; // Increase bitrate for low latency
        }
        
        // Adjust based on packet loss
        if packet_loss_percent > 5.0 {
            adjustment_factor *= 0.7; // Significant reduction for packet loss
        } else if packet_loss_percent > 1.0 {
            adjustment_factor *= 0.9;
        }
        
        // Adjust based on available bandwidth
        let available_bandwidth_bps = bandwidth_kbps * 1000.0;
        let current_usage_ratio = (self.current_bitrate as f32) / available_bandwidth_bps;
        
        if current_usage_ratio > 0.8 {
            adjustment_factor *= 0.8; // Reduce if using too much bandwidth
        } else if current_usage_ratio < 0.3 {
            adjustment_factor *= 1.2; // Increase if bandwidth is underutilized
        }
        
        // Apply adjustment
        self.target_bitrate = ((self.current_bitrate as f32) * adjustment_factor) as u32;
        self.target_bitrate = self.target_bitrate.clamp(self.min_bitrate, self.max_bitrate);
        
        if (self.target_bitrate as i32 - self.current_bitrate as i32).abs() > (self.current_bitrate / 10) as i32 {
            self.current_bitrate = self.target_bitrate;
            self.last_adjustment = Instant::now();
            
            info!("Adjusted bitrate to {} kbps (latency: {}ms, loss: {:.1}%, bandwidth: {:.0} kbps)",
                  self.current_bitrate / 1000, latency_ms, packet_loss_percent, bandwidth_kbps);
            
            Some(self.current_bitrate)
        } else {
            None
        }
    }
}
