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
    sync::{mpsc, broadcast},
    time,
};
use uuid::Uuid;
use base64::{Engine, engine::general_purpose};
use log::{debug, error, info, warn};

use super::models::NetworkStats;

// Helper function to make the future Send
pub async fn handle_socket_wrapper(socket: WebSocket, monitor: usize, codec: String, _enable_audio: bool) {
    // Force H.264 for WebRTC streaming only
    let codec = if codec == "h265" || codec == "av1" { 
        info!("Codec {} not supported yet, falling back to H.264", codec);
        "h264".to_string()
    } else if codec == "jpeg" {
        info!("JPEG codec is deprecated, using H.264 instead");
        "h264".to_string()
    } else {
        codec
    };
    
    handle_socket(socket, monitor, codec, _enable_audio).await;
}

// New helper function with stop signal
pub async fn handle_socket_wrapper_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    codec: String, 
    _enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    // Force H.264 for WebRTC streaming only
    let codec = if codec == "h265" || codec == "av1" { 
        info!("Codec {} not supported yet, falling back to H.264", codec);
        "h264".to_string()
    } else if codec == "jpeg" {
        info!("JPEG codec is deprecated, using H.264 instead");
        "h264".to_string()
    } else {
        codec
    };
    
    handle_socket_with_stop(socket, monitor, codec, _enable_audio, stop_rx).await;
}

pub async fn handle_socket(socket: WebSocket, monitor: usize, codec: String, enable_audio: bool) {
    info!("New WebSocket connection: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // For now, all codecs use the legacy handler since WebRTC has Send trait issues
    // TODO: Implement proper WebRTC streaming in separate thread
    handle_legacy_socket(socket, monitor, codec, enable_audio).await;
}

async fn handle_legacy_socket(socket: WebSocket, monitor: usize, codec: String, enable_audio: bool) {
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
                    info!("Falling back to JPEG encoding for better compatibility");
                    None
                }
            }
        } else {
            None
        };
        
        // Use delta encoding by default for JPEG mode
        let use_delta = selected_codec == "jpeg";
        
        // Network stats for adaptive streaming
        let _last_network_stats = NetworkStats {
            latency: 0,
            bandwidth: 5.0, // Default assumption: 5 Mbps
            packet_loss: 0.0,
        };
        
        // Frame counter for keyframes
        let mut frame_count = 0;
        
        // Performance metrics
        let mut _total_capture_time = Duration::from_secs(0);
        let mut _total_encode_time = Duration::from_secs(0);
        let mut _frames_processed = 0;
        
        // Capture loop
        loop {
            if selected_codec != "jpeg" && video_encoder.is_some() {
                // H.264/H.265/AV1 encoding
                if let Some(encoder) = &mut video_encoder {
                    // Measure capture time
                    let capture_start = Instant::now();
                    match screen_capturer.capture_raw() {
                        Ok(raw_frame) => {
                            // Force keyframe every 5 seconds or on poor network conditions
                            let force_keyframe = frame_count % 150 == 0;
                            
                            // Encode the frame
                            match encoder.encode_frame(&raw_frame, force_keyframe) {
                                Ok(encoded) => {
                                    _frames_processed += 1;
                                    
                                    // Send encoded frame as video_frame message
                                    if let Err(_) = encoded_tx.blocking_send(Ok(encoded)) {
                                        break;
                                    }
                                },
                                Err(e) => {
                                    // Check if this is a hardware encoder issue
                                    if e.to_string().contains("Hardware encoder failed to open") {
                                        error!("Hardware encoder failed ({}), switching to software encoder", e);
                                        
                                        // Try to recreate encoder with software encoding
                                        let software_config = EncoderConfig {
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
    
    // Set up audio capture
    let _audio_capturer_arc = if enable_audio {
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
    let _encryption_manager = Arc::new(EncryptionManager::new(&encryption_key));
    
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
    
    // Send monitor list info
    let monitor_list = serde_json::json!({
        "type": "monitors",
        "monitors": available_monitors.iter().map(|m| serde_json::json!({
            "id": m.id,
            "name": m.name,
            "width": m.width,
            "height": m.height,
            "position_x": m.position_x,
            "position_y": m.position_y,
            "is_primary": m.is_primary,
            "scale_factor": m.scale_factor
        })).collect::<Vec<_>>()
    });
    
    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();
    
    if let Err(e) = sender.send(Message::Text(server_info.to_string())).await {
        error!("Failed to send server info: {}", e);
        return;
    }
    
    // Send monitor list after server info
    if let Err(e) = sender.send(Message::Text(monitor_list.to_string())).await {
        error!("Failed to send monitor list: {}", e);
        return;
    }
    
    // Create a channel for input events
    let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(100);
    
    // Create a channel for network stats
    let (net_stats_tx, mut net_stats_rx) = mpsc::channel::<NetworkStats>(10);
    
    // Channels for general messaging
    let (message_tx, mut message_rx) = mpsc::channel::<String>(100);
    
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
                                _ => {
                                    debug!("Unknown message type: {}", msg_type);
                                }
                            }
                        }
                    }
                },
                Ok(Message::Binary(_)) => {
                    debug!("Received binary message (not implemented)");
                },
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed by client");
                    break;
                },
                Ok(Message::Ping(_)) => {
                    debug!("Received ping message");
                },
                Ok(Message::Pong(_)) => {
                    debug!("Received pong message");
                },
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
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
    
    // Main messaging loop
    let message_tx_for_frames = message_tx.clone();
    let target_fps = 30;
    let frame_interval = Duration::from_millis(1000 / target_fps);
    let mut last_frame = Instant::now();
    
    // Quality update timer
    let mut quality_update_timer = Instant::now();
    
    // Frame counter for WebRTC sequence numbers
    let mut frame_count: u64 = 0;
    
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
                    
                    // Create video frame message for better compatibility
                    let frame_msg = serde_json::json!({
                        "type": "video_frame",
                        "codec": codec,
                        "data": base64_data,
                        "is_keyframe": frame_count % 150 == 0, // Mark keyframes
                        "sequence_number": frame_count,
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
                    
                    frame_count += 1;
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
    
    // Wait for screen capture thread to finish
    let _ = screen_handle.join();
}

// New handler with stop signal
pub async fn handle_socket_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    codec: String, 
    _enable_audio: bool, 
    mut stop_rx: broadcast::Receiver<()>
) {
    info!("New WebSocket connection: monitor={}, codec={} (H.264 only)", monitor, codec);
    
    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();
    
    // Create channels for communication
    let (message_tx, mut message_rx) = mpsc::channel::<String>(100);
    let (encoded_tx, mut encoded_rx) = mpsc::channel::<Result<Vec<u8>, String>>(30);
    
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
        available_monitors.iter().position(|m| m.is_primary).unwrap_or(0)
    };
    
    // Setup H.264 screen capture in a separate thread
    let (thread_stop_tx, mut thread_stop_rx) = mpsc::channel::<()>(1);
    let screen_handle = std::thread::spawn(move || {
        // Initialize screen capture for the selected monitor
        let mut screen_capturer = match ScreenCapture::new(Some(monitor_index)) {
            Ok(capturer) => capturer,
            Err(e) => {
                let err_msg = format!("Failed to initialize screen capture: {}", e);
                let _ = encoded_tx.blocking_send(Err(err_msg));
                return;
            }
        };
        
        // Get screen dimensions
        let (width, height) = screen_capturer.dimensions();
        
        // Initialize H.264 encoder - force software for compatibility
        let encoder_config = EncoderConfig {
            width: width as u32,
            height: height as u32,
            bitrate: 2_000_000, // 2 Mbps default
            framerate: 30,
            keyframe_interval: 90, // Every 3 seconds at 30fps
            preset: "ultrafast".to_string(),
            use_hardware: false, // Force software for reliability
            codec_type: CodecType::H264,
        };
        
        let mut video_encoder = match VideoEncoder::new(encoder_config) {
            Ok(encoder) => {
                info!("Successfully initialized H.264 software encoder for {}x{} at 30 fps", width, height);
                encoder
            },
            Err(e) => {
                let err_msg = format!("Failed to initialize H.264 encoder: {}", e);
                error!("{}", err_msg);
                let _ = encoded_tx.blocking_send(Err(err_msg));
                return;
            }
        };
        
        // Frame counter for keyframes
        let mut frame_count = 0u64;
        
        // Main capture loop
        loop {
            // Check for stop signal (non-blocking)
            if thread_stop_rx.try_recv().is_ok() {
                info!("Screen capture thread received stop signal, exiting");
                break;
            }
            
            frame_count += 1;
            
            // Capture and encode frame
            match screen_capturer.capture_raw() {
                Ok(raw_frame) => {
                    // Force keyframe every 3 seconds
                    let force_keyframe = frame_count % 90 == 0;
                    
                    // Encode the frame
                    match video_encoder.encode_frame(&raw_frame, force_keyframe) {
                        Ok(encoded) => {
                            // Send encoded frame
                            if let Err(_) = encoded_tx.blocking_send(Ok(encoded)) {
                                break;
                            }
                        },
                        Err(e) => {
                            error!("H.264 encoding failed: {}", e);
                            let _ = encoded_tx.blocking_send(Err(format!("Encoding error: {}", e)));
                            break;
                        }
                    }
                },
                Err(e) => {
                    let err_msg = format!("Failed to capture frame: {}", e);
                    let _ = encoded_tx.blocking_send(Err(err_msg));
                    break;
                }
            }
            
            // Frame rate control - 30 FPS
            std::thread::sleep(Duration::from_millis(33));
        }
    });
    
    // Send server info
    let server_info = serde_json::json!({
        "type": "server_info",
        "width": available_monitors.get(monitor_index).map(|m| m.width).unwrap_or(1920),
        "height": available_monitors.get(monitor_index).map(|m| m.height).unwrap_or(1080),
        "monitor": monitor_index,
        "codec": "h264",
        "hostname": "Clever KVM",
        "audio": false,
        "encryption": false
    });
    
    if let Err(e) = message_tx.send(server_info.to_string()).await {
        error!("Failed to send server info: {}", e);
        return;
    }
    
    // Input handler
    let input_handler = Arc::new(Mutex::new(InputHandler::new()));
    
    // Handle incoming WebSocket messages
    let message_tx_for_ws = message_tx.clone();
    let input_handler_for_ws = input_handler.clone();
    let stop_rx2 = stop_rx.resubscribe();
    tokio::spawn(async move {
        let mut stop_rx = stop_rx2;
        loop {
            tokio::select! {
                _ = stop_rx.recv() => {
                    info!("WebSocket receiver received stop signal");
                    break;
                }
                
                msg = receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(json) => {
                                    if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
                                        match msg_type {
                                            "input" => {
                                                if let Ok(input_event) = serde_json::from_value::<InputEvent>(json) {
                                                    let mut handler = input_handler_for_ws.lock().unwrap();
                                                    if let Err(e) = handler.handle_event(input_event) {
                                                        error!("Failed to handle input event: {}", e);
                                                    }
                                                }
                                            },
                                            "ping" => {
                                                let pong = serde_json::json!({"type": "pong"});
                                                let _ = message_tx_for_ws.send(pong.to_string()).await;
                                            },
                                            _ => {
                                                debug!("Unknown message type: {}", msg_type);
                                            }
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("Failed to parse JSON message: {}", e);
                                }
                            }
                        },
                        Some(Ok(Message::Close(_))) => {
                            info!("WebSocket connection closed by client");
                            break;
                        },
                        Some(Err(e)) => {
                            error!("WebSocket error: {}", e);
                            break;
                        }
                        None => break,
                    }
                }
            }
        }
        
        info!("Client disconnected");
    });
    
    // Main frame streaming loop
    let message_tx_for_frames = message_tx.clone();
    let stop_rx3 = stop_rx.resubscribe();
    tokio::spawn(async move {
        let mut stop_rx = stop_rx3;
        let mut last_frame_time = Instant::now();
        let mut frame_sequence: u64 = 0;
        let target_frame_interval = Duration::from_millis(33); // ~30 FPS
        
        loop {
            tokio::select! {
                _ = stop_rx.recv() => {
                    info!("Frame handler received stop signal, stopping");
                    break;
                }
                
                result = encoded_rx.recv() => {
                    match result {
                        Some(Ok(frame_data)) => {
                            let now = Instant::now();
                            if now.duration_since(last_frame_time) >= target_frame_interval {
                                let frame_b64 = general_purpose::STANDARD.encode(&frame_data);
                                let message = serde_json::json!({
                                    "type": "video_frame",
                                    "codec": "h264",
                                    "data": frame_b64,
                                    "is_keyframe": frame_sequence % 90 == 0, // Keyframes every 3 seconds
                                    "sequence_number": frame_sequence,
                                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis()
                                }).to_string();
                                
                                if let Err(_) = message_tx_for_frames.send(message).await {
                                    break;
                                }
                                
                                frame_sequence += 1;
                                last_frame_time = now;
                            }
                        },
                        Some(Err(e)) => {
                            error!("Encoded frame error: {}", e);
                            break;
                        },
                        None => break,
                    }
                }
            }
        }
    });
    
    // Handle outgoing messages
    let stop_rx4 = stop_rx.resubscribe();
    tokio::spawn(async move {
        let mut stop_rx = stop_rx4;
        loop {
            tokio::select! {
                _ = stop_rx.recv() => {
                    info!("Message sender received stop signal");
                    break;
                }
                
                msg = message_rx.recv() => {
                    if let Some(msg) = msg {
                        if let Err(e) = sender.send(Message::Text(msg)).await {
                            error!("Failed to send WebSocket message: {}", e);
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    });
    
    // Wait for stop signal
    let _ = stop_rx.recv().await;
    
    // Signal thread to stop
    let _ = thread_stop_tx.send(()).await;
    
    // Wait for screen capture thread to finish
    let _ = screen_handle.join();
    
    info!("WebSocket connection cleanup completed");
}

async fn handle_legacy_socket_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    codec: String, 
    enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    info!("New WebSocket connection: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Create a thread-safe channel for screen data
    let (screen_tx, mut screen_rx) = mpsc::channel::<Result<Vec<u8>, String>>(10);
    let (delta_tx, mut delta_rx) = mpsc::channel::<Result<HashMap<usize, Vec<u8>>, String>>(10);
    let (encoded_tx, mut encoded_rx) = mpsc::channel::<Result<Vec<u8>, String>>(10);
    
    // Create a stop signal for the screen capture thread
    let (thread_stop_tx, mut thread_stop_rx) = mpsc::channel::<()>(1);
    
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
    
    // Setup screen capture in a separate thread with stop signal
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
                    info!("Falling back to JPEG encoding for better compatibility");
                    None
                }
            }
        } else {
            None
        };
        
        // Use delta encoding by default for JPEG mode
        let use_delta = selected_codec == "jpeg";
        
        // Frame counter for keyframes
        let mut frame_count = 0u64;
        let mut _frames_processed = 0u64;
        
        // Main capture loop with stop signal check
        loop {
            // Check for stop signal (non-blocking)
            if thread_stop_rx.try_recv().is_ok() {
                info!("Screen capture thread received stop signal, exiting");
                break;
            }
            
            frame_count += 1;
            
            if use_delta {
                // Delta-encoded JPEG capture for legacy clients
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
            } else if let Some(encoder) = &mut video_encoder {
                // H.264/H.265/AV1 encoding
                let _capture_start = Instant::now();
                match screen_capturer.capture_raw() {
                    Ok(raw_frame) => {
                        // Force keyframe every 5 seconds or on poor network conditions
                        let force_keyframe = frame_count % 150 == 0;
                        
                        // Encode the frame
                        match encoder.encode_frame(&raw_frame, force_keyframe) {
                            Ok(encoded) => {
                                _frames_processed += 1;
                                
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
                                    let software_config = EncoderConfig {
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
                                    warn!("Encoding failed: {}", e);
                                    // Sleep and continue for temporary errors
                                    std::thread::sleep(Duration::from_millis(16));
                                    continue;
                                }
                            }
                        }
                    },
                    Err(e) => {
                        warn!("Failed to capture raw frame: {}", e);
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                }
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
            }
            
            // Sleep to maintain target frame rate
            std::thread::sleep(Duration::from_millis(1000 / 30));
        }
        
        info!("Screen capture thread exiting");
    });
    
    // Continue with the rest of the function similar to handle_legacy_socket
    // but with proper cleanup handling...
    
    // Set up WebSocket communication and task spawning
    let (mut sender, mut receiver) = socket.split();
    let (message_tx, mut message_rx) = mpsc::channel::<String>(100);
    let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(100);
    
    // Initialize input handler
    let input_handler = Arc::new(Mutex::new(InputHandler::new()));
    
    // Create multiple receivers for the stop signal
    let mut stop_rx1 = stop_rx.resubscribe();
    let mut stop_rx2 = stop_rx.resubscribe();
    let mut stop_rx3 = stop_rx.resubscribe();
    
    // Spawn task to handle client messages with stop signal
    let input_tx_for_messages = input_tx.clone();
    let client_handler = tokio::spawn(async move {
        while let Some(msg_result) = receiver.next().await {
            // Check for stop signal
            if stop_rx1.try_recv().is_ok() {
                info!("WebSocket handler received stop signal, closing connection");
                break;
            }
            
            match msg_result {
                Ok(Message::Text(text)) => {
                    // Handle text messages (input events, etc.)
                    match serde_json::from_str::<InputEvent>(&text) {
                        Ok(event) => {
                            if let Err(e) = input_tx_for_messages.send(event).await {
                                error!("Failed to send input event: {}", e);
                                break;
                            }
                        },
                        Err(e) => {
                            debug!("Failed to parse input event: {} - text: {}", e, text);
                        }
                    }
                },
                Ok(Message::Binary(_)) => {
                    debug!("Received binary message (not implemented)");
                },
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed by client");
                    break;
                },
                Ok(Message::Ping(_)) => {
                    debug!("Received ping message");
                },
                Ok(Message::Pong(_)) => {
                    debug!("Received pong message");
                },
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
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
    
    // Main messaging loop
    let message_tx_for_frames = message_tx.clone();
    
    // Handle different frame types based on codec
    tokio::spawn(async move {
        let mut last_frame_time = Instant::now();
        let mut frame_sequence: u64 = 0;
        let target_frame_interval = Duration::from_millis(1000 / 30); // 30 FPS
        
        loop {
            tokio::select! {
                // Check for stop signal
                _ = stop_rx2.recv() => {
                    info!("Frame handler received stop signal, stopping");
                    break;
                }
                
                // Handle encoded frames (H.264/H.265/AV1)
                result = encoded_rx.recv() => {
                    match result {
                        Some(Ok(frame_data)) => {
                            let now = Instant::now();
                            if now.duration_since(last_frame_time) >= target_frame_interval {
                                let frame_b64 = general_purpose::STANDARD.encode(&frame_data);
                                let message = serde_json::json!({
                                    "type": "video_frame",
                                    "codec": codec.clone(),
                                    "data": frame_b64,
                                    "is_keyframe": frame_sequence % 150 == 0, // Mark keyframes every 5 seconds at 30fps
                                    "sequence_number": frame_sequence,
                                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis()
                                }).to_string();
                                
                                if let Err(_) = message_tx_for_frames.send(message).await {
                                    break;
                                }
                                
                                frame_sequence += 1;
                                last_frame_time = now;
                            }
                        },
                        Some(Err(e)) => {
                            error!("Encoded frame error: {}", e);
                            break;
                        },
                        None => break,
                    }
                }
                
                // Handle JPEG frames
                result = screen_rx.recv() => {
                    match result {
                        Some(Ok(frame_data)) => {
                            let now = Instant::now();
                            if now.duration_since(last_frame_time) >= target_frame_interval {
                                let frame_b64 = general_purpose::STANDARD.encode(&frame_data);
                                let message = serde_json::json!({
                                    "type": "frame",
                                    "data": frame_b64,
                                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis()
                                }).to_string();
                                
                                if let Err(_) = message_tx_for_frames.send(message).await {
                                    break;
                                }
                                
                                last_frame_time = now;
                            }
                        },
                        Some(Err(e)) => {
                            error!("Frame error: {}", e);
                            break;
                        },
                        None => break,
                    }
                }
                
                // Handle delta frames
                result = delta_rx.recv() => {
                    match result {
                        Some(Ok(tiles)) => {
                            let mut tile_data = HashMap::new();
                            for (tile_idx, tile_bytes) in tiles.iter() {
                                tile_data.insert(tile_idx.to_string(), general_purpose::STANDARD.encode(tile_bytes));
                            }
                            
                            let message = serde_json::json!({
                                "type": "delta_frame",
                                "tiles": tile_data,
                                "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis()
                            }).to_string();
                            
                            if let Err(_) = message_tx_for_frames.send(message).await {
                                break;
                            }
                        },
                        Some(Err(e)) => {
                            error!("Delta frame error: {}", e);
                            break;
                        },
                        None => break,
                    }
                }
            }
        }
    });
    
    // Main message sending loop
    tokio::spawn(async move {
        while let Some(message) = message_rx.recv().await {
            if let Err(e) = sender.send(Message::Text(message)).await {
                error!("Failed to send WebSocket message: {}", e);
                break;
            }
        }
    });
    
    // Wait for client handler to finish or stop signal
    tokio::select! {
        _ = client_handler => {
            info!("Client handler finished");
        }
        _ = stop_rx3.recv() => {
            info!("Received stop signal, terminating connection");
        }
    }
    
    // Send stop signal to screen capture thread
    let _ = thread_stop_tx.send(()).await;
    
    // Wait for screen capture thread to finish
    if let Err(e) = tokio::task::spawn_blocking(move || screen_handle.join()).await {
        error!("Failed to join screen capture thread: {:?}", e);
    }
    
    info!("WebSocket connection with stop signal finished cleanup");
}
