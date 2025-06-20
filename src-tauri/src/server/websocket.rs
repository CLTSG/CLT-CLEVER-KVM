use crate::capture::ScreenCapture;
use crate::input::{InputEvent, InputHandler};
use crate::utils::EncryptionManager;
use crate::codec::{VideoEncoder, EncoderConfig, CodecType}; // Add CodecType import
use crate::audio::{AudioCapturer, AudioConfig}; // Add AudioConfig import
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::{
    sync::mpsc,
    time,
};
use uuid::Uuid;
use base64::{Engine, engine::general_purpose};
use log::{debug, error, info, warn};

use super::models::NetworkStats;

// Helper function to make the future Send
pub async fn handle_socket_wrapper(socket: WebSocket, monitor: usize, codec: String, enable_audio: bool) {
    handle_socket(socket, monitor, codec, enable_audio).await;
}

pub async fn handle_socket(socket: WebSocket, monitor: usize, codec: String, enable_audio: bool) {
    info!("New WebSocket connection: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Create a thread-safe channel for screen data
    let (screen_tx, mut screen_rx) = mpsc::channel::<Result<Vec<u8>, String>>(10);
    let (delta_tx, mut delta_rx) = mpsc::channel::<Result<HashMap<usize, Vec<u8>>, String>>(10);
    let (encoded_tx, mut encoded_rx) = mpsc::channel::<Result<Vec<u8>, String>>(10);
    
    // Initialize monitors
    let available_monitors = match ScreenCapture::get_all_monitors() {
        Ok(monitors) => {
            info!("Found {} monitors", monitors.len());
            for (i, m) in monitors.iter().enumerate() {
                info!("Monitor {}: {} - {}x{} at ({},{}) Primary: {} Scale: {}",
                     i, m.name, m.width, m.height, m.position_x, m.position_y, 
                     m.is_primary, m.scale_factor);
            }
            monitors
        },
        Err(e) => {
            error!("Failed to get monitor info: {}", e);
            Vec::new()
        }
    };
    
    // Choose the appropriate monitor
    let monitor_index = if monitor < available_monitors.len() {
        monitor
    } else {
        warn!("Requested monitor {} not available, falling back to primary", monitor);
        // Find primary monitor or default to 0
        available_monitors.iter().position(|m| m.is_primary).unwrap_or(0)
    };
    
    // Setup screen capture in a separate thread
    let selected_codec = codec.clone();
    let screen_handle = std::thread::spawn(move || {
        // Initialize screen capture for the selected monitor
        let mut screen_capturer = match ScreenCapture::new(Some(monitor_index)) {
            Ok(capturer) => capturer,
            Err(e) => {
                let err_msg = format!("Failed to initialize screen capture: {}", e);
                let _ = screen_tx.blocking_send(Err(err_msg.clone()));
                let _ = encoded_tx.blocking_send(Err(err_msg));
                return;
            }
        };
        
        // Get screen dimensions for initial info
        let (width, height) = screen_capturer.dimensions();
        let (tile_width, tile_height, tile_size) = screen_capturer.tile_dimensions();
        
        // Send the dimensions
        let dimensions = (width, height, tile_width, tile_height, tile_size);
        let _ = screen_tx.blocking_send(Ok(bincode::serialize(&dimensions).unwrap_or_default()));
        
        // Default quality
        let current_quality = super::models::DEFAULT_QUALITY;
        
        // Convert codec string to enum
        let codec_type = CodecType::from_string(&selected_codec);
        
        // Initialize codec encoder if using H.264/H.265/AV1
        let mut video_encoder: Option<VideoEncoder> = if selected_codec != "jpeg" {
            // Create encoder configuration - default to software encoding
            let encoder_config = EncoderConfig {
                width: width as u32,
                height: height as u32,
                bitrate: 2_000_000, // 2 Mbps default
                framerate: 30,
                keyframe_interval: 60, // Every 2 seconds at 30fps
                preset: "ultrafast".to_string(), // This will be used for software encoders
                use_hardware: false, // Default to software encoding
                codec_type,
            };
            
            // Try software encoder first
            match VideoEncoder::new(encoder_config.clone()) {
                Ok(encoder) => {
                    info!("Successfully initialized {:?} software encoder for {}x{} at {} fps", 
                         codec_type, width, height, 30);
                    Some(encoder)
                },
                Err(e) => {
                    warn!("Software encoder initialization failed: {}", e);
                    
                    // Only try hardware as fallback if software fails
                    let mut hardware_config = encoder_config.clone();
                    hardware_config.use_hardware = true;
                    
                    match VideoEncoder::new(hardware_config) {
                        Ok(encoder) => {
                            info!("Successfully initialized {:?} hardware encoder for {}x{} at {} fps", 
                                 codec_type, width, height, 30);
                            Some(encoder)
                        },
                        Err(e) => {
                            error!("Both hardware and software encoders failed: {}", e);
                            info!("Falling back to JPEG encoding for better compatibility");
                            None
                        }
                    }
                }
            }
        } else {
            None
        };
        
        // Use delta encoding by default for JPEG mode
        let use_delta = selected_codec == "jpeg";
        
        // Network stats for adaptive streaming
        let last_network_stats = NetworkStats {
            latency: 0,
            bandwidth: 5.0, // Default assumption: 5 Mbps
            packet_loss: 0.0,
        };
        
        // Frame counter for keyframes
        let mut frame_count = 0;
        
        // Performance metrics
        let mut total_capture_time = Duration::from_secs(0);
        let mut total_encode_time = Duration::from_secs(0);
        let mut frames_processed = 0;
        let mut _last_metrics_report = Instant::now(); // Mark as intentionally unused
        
        // Capture loop
        loop {
            if selected_codec != "jpeg" && video_encoder.is_some() {
                // H.264/H.265/AV1 encoding
                if let Some(encoder) = &mut video_encoder {
                    // Measure capture time
                    let capture_start = Instant::now();
                    match screen_capturer.capture_raw() {
                        Ok(raw_frame) => {
                            let capture_time = capture_start.elapsed();
                            total_capture_time += capture_time;
                            
                            // Force keyframe every 5 seconds or on poor network conditions
                            let force_keyframe = frame_count % 150 == 0 || 
                                               (last_network_stats.packet_loss > 5.0 && frame_count % 30 == 0);
                            
                            // Measure encoding time
                            let encode_start = Instant::now();
                            
                            // Encode the frame
                            match encoder.encode_frame(&raw_frame, force_keyframe) {
                                Ok(encoded) => {
                                    let encode_time = encode_start.elapsed();
                                    total_encode_time += encode_time;
                                    frames_processed += 1;
                                    
                                    // Send encoded frame
                                    if let Err(_) = encoded_tx.blocking_send(Ok(encoded)) {
                                        break;
                                    }
                                },
                                Err(e) => {
                                    // Check if this is a hardware encoder issue
                                    if e.to_string().contains("Hardware encoder failed to open") {
                                        error!("Hardware encoder failed ({}), switching to software encoder", e);
                                        
                                        // Try to recreate encoder with software encoding
                                        let mut software_config = EncoderConfig {
                                            width: width as u32,
                                            height: height as u32,
                                            bitrate: 2_000_000,
                                            framerate: 30,
                                            keyframe_interval: 60,
                                            preset: "ultrafast".to_string(),
                                            use_hardware: false, // Force software
                                            codec_type,
                                        };
                                        
                                        match VideoEncoder::new(software_config) {
                                            Ok(new_encoder) => {
                                                info!("Successfully switched to software encoder");
                                                *encoder = new_encoder;
                                                // Try encoding again with the new software encoder
                                                continue;
                                            },
                                            Err(e) => {
                                                error!("Software encoder also failed: {}, falling back to JPEG", e);
                                                // Clear the encoder to force JPEG fallback
                                                video_encoder = None;
                                                break;
                                            }
                                        }
                                    } else {
                                        // Other encoding errors, fall back to JPEG
                                        error!("Encoding failed: {}, falling back to JPEG", e);
                                        
                                        // Try JPEG encoding as fallback
                                        match screen_capturer.capture_jpeg(current_quality) {
                                            Ok(jpeg_data) => {
                                                if let Err(_) = screen_tx.blocking_send(Ok(jpeg_data)) {
                                                    break;
                                                }
                                            },
                                            Err(jpeg_err) => {
                                                error!("JPEG fallback also failed: {}", jpeg_err);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            
                            frame_count += 1;
                        },
                        Err(e) => {
                            let err_msg = format!("Failed to capture raw frame: {}", e);
                            let _ = encoded_tx.blocking_send(Err(err_msg));
                            break;
                        }
                    }
                    
                    // Frame rate control
                    let frame_time = capture_start.elapsed();
                    let target_frame_time = Duration::from_millis(1000 / 30);
                    
                    if frame_time < target_frame_time {
                        std::thread::sleep(target_frame_time - frame_time);
                    }
                }
            } else if use_delta {
                // JPEG with delta encoding
                match screen_capturer.capture_jpeg_delta(Some(current_quality)) {
                    Ok(tiles) => {
                        if let Err(_) = delta_tx.blocking_send(Ok(tiles)) {
                            break;
                        }
                    },
                    Err(e) => {
                        let err_msg = format!("Failed to capture delta: {}", e);
                        let _ = delta_tx.blocking_send(Err(err_msg));
                        break;
                    }
                }
                
                // Sleep to maintain target frame rate
                std::thread::sleep(Duration::from_millis(1000 / 30));
            } else {
                // Basic JPEG encoding
                match screen_capturer.capture_jpeg(current_quality) {
                    Ok(jpeg_data) => {
                        if let Err(_) = screen_tx.blocking_send(Ok(jpeg_data)) {
                            break;
                        }
                    },
                    Err(e) => {
                        let err_msg = format!("Failed to capture jpeg: {}", e);
                        let _ = screen_tx.blocking_send(Err(err_msg));
                        break;
                    }
                }
                
                // Sleep to maintain target frame rate
                std::thread::sleep(Duration::from_millis(1000 / 30));
            }
        }
    });
    
    // Set up audio capture with improved configuration
    let audio_capturer_arc = if enable_audio {
        let audio_config = AudioConfig {
            sample_rate: 48000,
            channels: 2,
            bit_depth: 16,
            opus_bitrate: 128000,  // 128 kbps for high quality
            echo_cancellation: true,
            noise_suppression: true,
        };
        
        match AudioCapturer::new(audio_config) {
            Ok(mut capturer) => {
                // Initialize WebRTC
                if let Err(e) = capturer.initialize_webrtc().await {
                    error!("Failed to initialize WebRTC audio: {}", e);
                    None
                } else {
                    Some(Arc::new(Mutex::new(capturer)))
                }
            },
            Err(e) => {
                error!("Failed to initialize audio capturer: {}", e);
                None
            }
        }
    } else {
        None
    };
    
    // Setup input handler with monitor information
    let mut input_handler = InputHandler::new();
    
    // Configure input handler with monitor positions
    let monitor_configs: Vec<(String, i32, i32, i32, i32)> = available_monitors.iter()
        .map(|m| (
            m.id.clone(),
            m.position_x,
            m.position_y,
            m.width as i32,
            m.height as i32
        ))
        .collect();
    
    input_handler.update_monitors(monitor_configs);
    
    // Set active monitor
    if let Some(monitor) = available_monitors.get(monitor_index) {
        if let Err(e) = input_handler.set_active_monitor(&monitor.id) {
            warn!("Failed to set active monitor: {}", e);
        }
    }
    
    let input_handler = Arc::new(Mutex::new(input_handler));
    
    // Get hostname
    let hostname = std::env::var("HOSTNAME")
        .unwrap_or_else(|_| "Unknown".to_string());
    
    // Setup encryption (optional)
    let encryption_key = format!("clever-kvm-{}", Uuid::new_v4());
    let encryption_manager = Arc::new(EncryptionManager::new(&encryption_key));
    
    // Get the initial dimensions from the capture thread
    let dimensions = match screen_rx.recv().await {
        Some(Ok(data)) => {
            match bincode::deserialize::<(usize, usize, usize, usize, usize)>(&data) {
                Ok(dims) => dims,
                Err(_) => {
                    error!("Failed to deserialize screen dimensions");
                    return;
                }
            }
        },
        Some(Err(e)) => {
            error!("{}", e);
            return;
        },
        None => {
            error!("Screen capture thread terminated unexpectedly");
            return;
        }
    };
    
    let (width, height, tile_width, tile_height, tile_size) = dimensions;
    
    // Send initial server info
    let selected_monitor = available_monitors.get(monitor_index)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Default".to_string());
    
    let server_info = serde_json::json!({
        "type": "info",
        "width": width,
        "height": height,
        "hostname": hostname,
        "monitor": selected_monitor,
        "monitor_index": monitor_index,
        "codec": codec,
        "tile_width": tile_width,
        "tile_height": tile_height,
        "tile_size": tile_size,
        "encryption": encryption_key,
        "audio": enable_audio
    });
    
    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();
    
    if let Err(e) = sender.send(Message::Text(server_info.to_string())).await {
        error!("Failed to send server info: {}", e);
        return;
    }
    
    // Create a channel for input events
    let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(100);
    
    // Create a channel for network stats
    let (net_stats_tx, mut net_stats_rx) = mpsc::channel::<NetworkStats>(10);
    
    // Channel for WebRTC signaling messages
    let (rtc_tx, mut rtc_rx) = mpsc::channel::<String>(10);
    
    // Channels for general messaging
    let (message_tx, mut message_rx) = mpsc::channel::<String>(100);
    
    // If audio is enabled, set up WebRTC signaling
    if let Some(audio_cap_arc) = &audio_capturer_arc {
        // Create WebRTC offer (safely using tokio::task::spawn_blocking)
        let audio_cap_arc_clone = audio_cap_arc.clone();
        let audio_offer = tokio::task::spawn_blocking(move || {
            let audio_cap = audio_cap_arc_clone.lock().unwrap();
            let rt = tokio::runtime::Handle::current();
            rt.block_on(audio_cap.create_offer())
        }).await.unwrap_or_else(|e| {
            error!("Failed to create WebRTC offer in blocking task: {}", e);
            Err("Task join error".to_string())
        });
        
        match audio_offer {
            Ok(offer) => {
                let offer_msg = serde_json::json!({
                    "type": "webrtc_offer",
                    "sdp": offer
                });
                
                if let Err(e) = message_tx.send(offer_msg.to_string()).await {
                    error!("Failed to send WebRTC offer: {}", e);
                }
            },
            Err(e) => {
                error!("Failed to create WebRTC offer: {}", e);
            }
        }
    }
    
    // Spawn a task to handle outgoing messages
    tokio::spawn(async move {
        while let Some(msg) = message_rx.recv().await {
            if let Err(e) = sender.send(Message::Text(msg)).await {
                error!("Failed to send message: {}", e);
                break;
            }
        }
    });
    
    // Spawn a task to handle incoming messages
    let message_tx_clone = message_tx.clone();
    let rtc_tx_clone = rtc_tx.clone();
    
    tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Parse message based on type
                    if let Ok(event) = serde_json::from_str::<InputEvent>(&text) {
                        // Handle input event
                        if let Err(e) = input_tx.send(event).await {
                            error!("Failed to send input event: {}", e);
                            break;
                        }
                    } else if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                        // Check message type
                        if let Some(msg_type) = value.get("type").and_then(|t| t.as_str()) {
                            match msg_type {
                                "ping" => {
                                    // Handle ping
                                    let response = serde_json::json!({
                                        "type": "ping",
                                        "timestamp": SystemTime::now()
                                            .duration_since(SystemTime::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis()
                                    });
                                    
                                    if let Err(e) = message_tx_clone.send(response.to_string()).await {
                                        error!("Failed to send ping response: {}", e);
                                    }
                                },
                                "network_stats" => {
                                    // Handle network stats
                                    let latency = value.get("latency").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
                                    let bandwidth = value.get("bandwidth").and_then(|b| b.as_f64()).unwrap_or(0.0) as f32;
                                    let packet_loss = value.get("packet_loss").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32;
                                    
                                    debug!("Network stats - Latency: {}ms, Bandwidth: {}Mbps, Packet loss: {}%", 
                                           latency, bandwidth, packet_loss);
                                    
                                    if let Err(e) = net_stats_tx.send(NetworkStats {
                                        latency,
                                        bandwidth,
                                        packet_loss,
                                    }).await {
                                        error!("Failed to send network stats: {}", e);
                                    }
                                },
                                "webrtc_answer" => {
                                    // Handle WebRTC answer for audio
                                    if let Some(sdp) = value.get("sdp").and_then(|s| s.as_str()) {
                                        if let Err(e) = rtc_tx_clone.send(sdp.to_string()).await {
                                            error!("Failed to send WebRTC answer: {}", e);
                                        }
                                    }
                                },
                                _ => {
                                    debug!("Unknown message type: {}", msg_type);
                                }
                            }
                        }
                    }
                },
                Ok(Message::Binary(bin)) => {
                    // Handle binary messages (potential for encrypted data)
                    if bin.len() > 0 {
                        // First byte could indicate message type
                        match bin[0] {
                            1 => {
                                // Encrypted input event
                                if let Ok(decrypted) = encryption_manager.decrypt(&bin[1..]) {
                                    if let Ok(event) = serde_json::from_slice::<InputEvent>(&decrypted) {
                                        if let Err(e) = input_tx.send(event).await {
                                            error!("Failed to send decrypted input event: {}", e);
                                        }
                                    }
                                }
                            },
                            _ => {
                                debug!("Unknown binary message type: {}", bin[0]);
                            }
                        }
                    }
                },
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed by client");
                    break;
                },
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                },
                _ => {}
            }
        }
        
        info!("Client disconnected");
    });
    
    // Process input events
    let input_handler_for_events = input_handler.clone();
    tokio::spawn(async move {
        while let Some(event) = input_rx.recv().await {
            let mut handler = input_handler_for_events.lock().unwrap();
            if let Err(e) = handler.handle_event(event) {
                error!("Failed to handle input event: {}", e);
            }
        }
    });
    
    // Handle WebRTC signaling for audio
    if let Some(audio_cap_arc) = &audio_capturer_arc {
        let rtc_audio_cap = audio_cap_arc.clone();
        
        tokio::spawn(async move {
            // Start audio capture (using spawn_blocking for MutexGuard)
            let audio_cap_clone = rtc_audio_cap.clone();
            tokio::task::spawn_blocking(move || {
                let audio_cap = audio_cap_clone.lock().unwrap();
                let rt = tokio::runtime::Handle::current();
                if let Err(e) = rt.block_on(audio_cap.start_capture()) {
                    error!("Failed to start audio capture: {}", e);
                }
            });
            
            // Process WebRTC answers
            while let Some(answer_sdp) = rtc_rx.recv().await {
                let audio_cap_arc_clone = rtc_audio_cap.clone();
                let answer_sdp_clone = answer_sdp.clone();
                
                // Use spawn_blocking for MutexGuard
                let result = tokio::task::spawn_blocking(move || {
                    let audio_cap = audio_cap_arc_clone.lock().unwrap();
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(audio_cap.set_remote_answer(answer_sdp_clone))
                }).await.unwrap_or_else(|e| {
                    error!("Task to set remote answer failed: {}", e);
                    Err("Task join error".to_string())
                });
                
                if let Err(e) = result {
                    error!("Failed to set remote answer: {}", e);
                } else {
                    info!("WebRTC connection established for audio");
                }
            }
        });
    }
    
    // Main messaging loop
    let message_tx_for_frames = message_tx.clone();
    let target_fps = 30;
    let frame_interval = Duration::from_millis(1000 / target_fps);
    let mut last_frame = Instant::now();
    
    // Quality update timer
    let mut quality_update_timer = Instant::now();
    
    // Main streaming loop
    loop {
        // Check for network stats updates
        if let Ok(stats) = net_stats_rx.try_recv() {
            // Use network stats to update quality
            
            // Send quality update every 5 seconds
            if quality_update_timer.elapsed() > Duration::from_secs(5) {
                // Calculate adaptive quality based on network conditions
                let mut quality = super::models::DEFAULT_QUALITY;
                
                if stats.latency > 200 || stats.packet_loss > 5.0 {
                    quality = (super::models::DEFAULT_QUALITY as i32 - 15)
                        .max(super::models::MIN_QUALITY as i32) as u8;
                } else if stats.latency < 50 && stats.bandwidth > 5.0 {
                    quality = (super::models::DEFAULT_QUALITY as i32 + 10)
                        .min(super::models::MAX_QUALITY as i32) as u8;
                }
                
                // Send quality update to client
                let quality_msg = serde_json::json!({
                    "type": "quality",
                    "value": quality
                });
                
                if let Err(e) = message_tx_for_frames.send(quality_msg.to_string()).await {
                    error!("Failed to send quality update: {}", e);
                    break;
                }
                
                quality_update_timer = Instant::now();
            }
        }
        
        // Handle different codecs
        if codec == "h264" || codec == "h265" {
            // Process H.264/H.265 encoded frames
            match encoded_rx.recv().await {
                Some(Ok(encoded_data)) => {
                    // Base64 encode the data
                    let base64_data = general_purpose::STANDARD.encode(&encoded_data);
                    
                    // Create frame message
                    let frame_msg = serde_json::json!({
                        "type": "video_frame",
                        "codec": codec,
                        "data": base64_data,
                        "timestamp": SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    });
                    
                    // Send the frame
                    if let Err(e) = message_tx_for_frames.send(frame_msg.to_string()).await {
                        error!("Failed to send encoded frame: {}", e);
                        break;
                    }
                },
                Some(Err(e)) => {
                    error!("Encoder error: {}", e);
                    break;
                },
                None => {
                    error!("Encoder channel closed");
                    break;
                }
            }
        } else if codec == "jpeg" {
            // Process delta JPEG frames
            match delta_rx.recv().await {
                Some(Ok(tiles)) => {
                    if !tiles.is_empty() {
                        // Convert tiles to base64
                        let mut base64_tiles = HashMap::new();
                        
                        for (idx, jpeg_data) in tiles {
                            base64_tiles.insert(idx, general_purpose::STANDARD.encode(&jpeg_data));
                        }
                        
                        // Create delta frame message
                        let delta_msg = serde_json::json!({
                            "type": "delta",
                            "tiles": base64_tiles,
                            "timestamp": SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis()
                        });
                        
                        // Send the frame
                        if let Err(e) = message_tx_for_frames.send(delta_msg.to_string()).await {
                            error!("Failed to send delta frame: {}", e);
                            break;
                        }
                    }
                },
                Some(Err(e)) => {
                    error!("Delta encoder error: {}", e);
                    break;
                },
                None => {
                    error!("Delta encoder channel closed");
                    break;
                }
            }
        } else {
            // Process full JPEG frames
            match screen_rx.recv().await {
                Some(Ok(jpeg_data)) => {
                    // Base64 encode the data
                    let base64_data = general_purpose::STANDARD.encode(&jpeg_data);
                    
                    // Create frame message
                    let frame_msg = serde_json::json!({
                        "type": "frame",
                        "width": width,
                        "height": height,
                        "image": base64_data,
                        "timestamp": SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    });
                    
                    // Send the frame
                    if let Err(e) = message_tx_for_frames.send(frame_msg.to_string()).await {
                        error!("Failed to send JPEG frame: {}", e);
                        break;
                    }
                },
                Some(Err(e)) => {
                    error!("JPEG encoder error: {}", e);
                    break;
                },
                None => {
                    error!("JPEG encoder channel closed");
                    break;
                }
            }
        }
        
        // Rate limiting
        let elapsed = last_frame.elapsed();
        if elapsed < frame_interval {
            time::sleep(frame_interval - elapsed).await;
        }
        last_frame = Instant::now();
    }
    
    // Clean up
    info!("Cleaning up WebSocket connection resources");
    
    // Stop audio capture if it was started
    if let Some(audio_cap_arc) = &audio_capturer_arc {
        let audio_cap = audio_cap_arc.lock().unwrap();
        if let Err(e) = audio_cap.stop_capture() {
            error!("Failed to stop audio capture: {}", e);
        }
    }
    
    // Wait for screen capture thread to finish
    let _ = screen_handle.join();
}