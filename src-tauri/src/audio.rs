use std::sync::{Arc, Mutex};
use webrtc::api::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType, RTCRtpCodecParameters};
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::media::Sample;
use log::{debug, error, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, Duration};

pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u8,
    pub bit_depth: u8,
    pub opus_bitrate: u32,  // Bitrate for Opus encoding
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            bit_depth: 16,
            opus_bitrate: 128000,  // 128 kbps default
            echo_cancellation: true,
            noise_suppression: true,
        }
    }
}

pub struct AudioCapturer {
    // WebRTC context
    peer_connection: Option<RTCPeerConnection>,
    audio_track: Option<Arc<TrackLocalStaticSample>>,
    // Audio capture parameters
    sample_rate: u32,
    channels: u8,
    bit_depth: u8,
    opus_bitrate: u32,
    echo_cancellation: bool,
    noise_suppression: bool,
    is_capturing: Arc<Mutex<bool>>,
}

impl AudioCapturer {
    pub fn new(config: AudioConfig) -> Result<Self, String> {
        info!("Initializing audio capturer with: {}Hz, {} channels, {} bit, {} kbps",
              config.sample_rate, config.channels, config.bit_depth, config.opus_bitrate/1000);
        
        Ok(Self {
            peer_connection: None,
            audio_track: None,
            sample_rate: config.sample_rate,
            channels: config.channels,
            bit_depth: config.bit_depth,
            opus_bitrate: config.opus_bitrate,
            echo_cancellation: config.echo_cancellation,
            noise_suppression: config.noise_suppression,
            is_capturing: Arc::new(Mutex::new(false)),
        })
    }
    
    pub async fn initialize_webrtc(&mut self) -> Result<(), String> {
        // Create a MediaEngine object to configure the supported codec
        let mut m = MediaEngine::default();
        
        // Register Opus codec for audio
        m.register_default_codecs()
            .map_err(|e| format!("Failed to register default codecs: {}", e))?;
        
        // Create the API object with the MediaEngine
        let api = APIBuilder::new()
            .with_media_engine(m)
            .build();
        
        // Create a new RTCPeerConnection with STUN servers
        let config = RTCConfiguration {
            ice_servers: vec![
                RTCIceServer {
                    urls: vec!["stun:stun.l.google.com:19302".to_string()],
                    ..Default::default()
                },
                RTCIceServer {
                    urls: vec!["stun:stun1.l.google.com:19302".to_string()],
                    ..Default::default()
                },
            ],
            ice_transport_policy: RTCIceTransportPolicy::All,
            ..Default::default()
        };
        
        let peer_connection = api.new_peer_connection(config)
            .await
            .map_err(|e| format!("Failed to create peer connection: {}", e))?;
        
        // Create a new audio track
        let audio_track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_OPUS.to_string(),
                clock_rate: self.sample_rate,
                channels: self.channels as u16,
                sdp_fmtp_line: format!("minptime=10;useinbandfec=1;stereo={}",
                                      if self.channels > 1 { "1" } else { "0" }),
                ..Default::default()
            },
            "audio".to_string(),
            "clever-kvm".to_string(),
        ));
        
        // Add the audio track to the peer connection
        let _rtp_sender = peer_connection.add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .map_err(|e| format!("Failed to add audio track: {}", e))?;
        
        self.peer_connection = Some(peer_connection);
        self.audio_track = Some(audio_track);
        
        info!("WebRTC audio initialized with {} channels at {} Hz", self.channels, self.sample_rate);
        Ok(())
    }
    
    pub async fn create_offer(&self) -> Result<String, String> {
        if let Some(pc) = &self.peer_connection {
            // Create an offer
            let offer = pc.create_offer(None)
                .await
                .map_err(|e| format!("Failed to create offer: {}", e))?;
            
            // Set the local description
            pc.set_local_description(offer.clone())
                .await
                .map_err(|e| format!("Failed to set local description: {}", e))?;
            
            // Wait for ICE gathering to complete
            // Don't try to clone RTCPeerConnection as it doesn't implement Clone
            let gathered = Arc::new(AtomicBool::new(false));
            let gathered_clone = gathered.clone();
            let (tx, rx) = tokio::sync::oneshot::channel();
            let tx = Arc::new(Mutex::new(Some(tx)));
            
            pc.on_ice_gathering_state_change(Box::new(move |state| {
                if state.to_string() == "complete" && !gathered_clone.load(Ordering::Relaxed) {
                    gathered_clone.store(true, Ordering::Relaxed);
                    
                    // Use the Mutex-wrapped tx to avoid ownership issues
                    if let Some(sender) = tx.lock().unwrap().take() {
                        let _ = sender.send(());
                    }
                    
                    Box::pin(async {})
                } else {
                    Box::pin(async {})
                }
            }));
            
            // Wait for ICE gathering with a timeout
            tokio::select! {
                _ = rx => {},
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                    warn!("ICE gathering timed out");
                }
            }
            
            // Get the SDP - use the original pc reference
            if let Some(local_desc) = pc.local_description().await {
                return Ok(local_desc.sdp);
            }
            
            return Err("Failed to get local description".to_string());
        }
        
        Err("WebRTC not initialized".to_string())
    }
    
    pub async fn set_remote_answer(&self, sdp: String) -> Result<(), String> {
        if let Some(pc) = &self.peer_connection {
            // Use answer method to create a valid session description
            let answer = webrtc::peer_connection::sdp::session_description::RTCSessionDescription::answer(sdp)
                .map_err(|e| format!("Failed to create answer: {}", e))?;
            
            pc.set_remote_description(answer)
                .await
                .map_err(|e| format!("Failed to set remote description: {}", e))?;
            
            info!("WebRTC connection established");
            Ok(())
        } else {
            Err("WebRTC not initialized".to_string())
        }
    }
    
    pub async fn start_capture(&self) -> Result<(), String> {
        {
            let mut is_capturing = self.is_capturing.lock().unwrap();
            if *is_capturing {
                return Ok(());
            }
            *is_capturing = true;
        }
        
        info!("Starting audio capture with Opus encoding at {} kbps", self.opus_bitrate/1000);
        
        // Improved audio capture implementation
        if let Some(audio_track) = &self.audio_track {
            let track = Arc::clone(audio_track);
            let is_capturing = Arc::clone(&self.is_capturing);
            let sample_rate = self.sample_rate;
            let channels = self.channels;
            let _opus_bitrate = self.opus_bitrate;  // Fix: Mark as unused with underscore
            
            tokio::spawn(async move {
                // Calculate frame size based on 20ms chunks (standard for Opus)
                let samples_per_frame = (sample_rate as f32 * 0.02) as usize; // 20ms
                let bytes_per_sample = 2; // 16-bit audio
                
                // In a real implementation, we would use a proper audio capture library
                // and Opus encoder. This is a placeholder that generates silence.
                let silence_frame = vec![0u8; samples_per_frame * channels as usize * bytes_per_sample];
                
                let mut packet_timestamp: u32 = 0;
                let frame_duration = std::time::Duration::from_millis(20);
                
                // For timing accuracy
                let mut last_frame_time = std::time::Instant::now();
                
                while *is_capturing.lock().unwrap() {
                    // In a real implementation:
                    // 1. Capture raw audio from system
                    // 2. Apply echo cancellation and noise suppression if enabled
                    // 3. Encode with Opus at the specified bitrate
                    
                    // Send the frame
                    let sample = Sample {
                        data: silence_frame.clone().into(),
                        duration: frame_duration,
                        ..Default::default()
                    };
                    
                    packet_timestamp = packet_timestamp.wrapping_add(samples_per_frame as u32);
                    
                    if let Err(e) = track.write_sample(&sample).await {
                        error!("Failed to write audio sample: {}", e);
                        break;
                    }
                    
                    // Precise timing for consistent audio frames
                    let elapsed = last_frame_time.elapsed();
                    if elapsed < frame_duration {
                        tokio::time::sleep(frame_duration - elapsed).await;
                    }
                    last_frame_time = std::time::Instant::now();
                }
                
                debug!("Audio capture stopped");
            });
        }
        
        Ok(())
    }
    
    pub fn stop_capture(&self) -> Result<(), String> {
        let mut is_capturing = self.is_capturing.lock().unwrap();
        *is_capturing = false;
        info!("Stopping audio capture");
        Ok(())
    }
    
    pub async fn close(&mut self) -> Result<(), String> {
        self.stop_capture()?;
        
        if let Some(pc) = &self.peer_connection {
            if let Err(e) = pc.close().await {
                error!("Error closing peer connection: {}", e);
            }
        }
        
        self.peer_connection = None;
        self.audio_track = None;
        
        Ok(())
    }
}
