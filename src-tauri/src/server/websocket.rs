use crate::capture::ScreenCapture;
use crate::input::{InputEvent, InputHandler};
use crate::utils::EncryptionManager;
use crate::codec::{VideoEncoder, EncoderConfig, CodecType}; // Add CodecType import
use crate::audio::{AudioCapturer, AudioConfig}; // Add AudioConfig import
use crate::server::webrtc_handler::{QualityProfile, StreamingControl, EncodedFrameMessage}; // Add WebRTC imports
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
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use gethostname;

use super::models::NetworkStats;

// Control messages for WebSocket communication
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ControlMessage {
    #[serde(rename = "ping")]
    Ping { timestamp: Option<u64> },
    
    #[serde(rename = "switch_codec")]
    SwitchCodec { codec: String },
    
    #[serde(rename = "request_keyframe")]
    RequestKeyframe,
    
    #[serde(rename = "quality_setting")]
    QualitySetting { quality: u8 },
    
    #[serde(rename = "network_stats")]
    NetworkStats { 
        latency: Option<u32>,
        bandwidth: Option<f32>,
        packet_loss: Option<f32>
    },
}

// Helper function to make the future Send
pub async fn handle_socket_wrapper(socket: WebSocket, monitor: usize, _codec: String, enable_audio: bool) {
    // Always use VP8 codec
    let codec = "vp8".to_string();
    
    // Extract connection info for logging
    info!("New WebSocket connection established - Monitor: {}, Codec: VP8 (forced), Audio: {}", 
          monitor, enable_audio);
    
    handle_socket(socket, monitor, codec.clone(), enable_audio).await;
    
    info!("WebSocket connection closed - Monitor: {}, Codec: VP8", monitor);
}

// New helper function with stop signal
pub async fn handle_socket_wrapper_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    _codec: String, 
    enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    // Always use VP8 codec
    let codec = "vp8".to_string();
        // Use WebRTC H.264 streaming for optimal performance
    let codec = if codec == "vp8" || codec == "webrtc" {
        "webrtc".to_string()
    } else {
        "vp8".to_string()  // Default to VP8
    };
    
    handle_socket_with_stop(socket, monitor, codec, enable_audio, stop_rx).await;
}

pub async fn handle_socket(socket: WebSocket, monitor: usize, codec: String, enable_audio: bool) {
    info!("New WebSocket connection: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Use WebRTC streaming for H.264 and optimal performance
    if codec == "webrtc" {
        handle_webrtc_socket(socket, monitor, enable_audio).await;
    } else {
        // Fallback to legacy for other codecs
        handle_legacy_socket(socket, monitor, "vp8".to_string(), enable_audio).await;
    }
}

async fn handle_legacy_socket(socket: WebSocket, monitor: usize, _codec: String, enable_audio: bool) {
    let codec = "vp8".to_string(); // Always use VP8
    info!("New WebSocket connection: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Create a thread-safe channel for screen data
    let (screen_tx, mut screen_rx) = mpsc::channel::<Result<Vec<u8>, String>>(10);
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
    let selected_codec = "vp8".to_string(); // Always use VP8
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
        
        // Initialize codec encoder for WebRTC VP8
        let mut video_encoder: Option<VideoEncoder> = {
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
            
            info!("Attempting to initialize video encoder with config: {}x{} @ {}fps, codec: {:?}", 
                  encoder_config.width, encoder_config.height, encoder_config.framerate, encoder_config.codec_type);
            
            // Try software encoder first
            match VideoEncoder::new(encoder_config.clone()) {
                Ok(encoder) => {
                    info!("Successfully initialized {:?} software encoder for {}x{} at {} fps", 
                         codec_type, width, height, 30);
                    Some(encoder)
                },
                Err(e) => {
                    error!("Software encoder initialization failed: {}", e);
                    error!("WebRTC H.264 encoder is required for operation");
                    
                    // Send error message to client
                    let error_msg = serde_json::json!({
                        "type": "error",
                        "message": format!("Failed to initialize video encoder: {}", e)
                    });
                    let _ = encoded_tx.blocking_send(Err(format!("Encoder initialization failed: {}", e)));
                    None
                }
            }
        };
        
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
            if let Some(encoder) = &mut video_encoder {
                // WebRTC H.264 encoding
                // Measure capture time
                let capture_start = Instant::now();
                match screen_capturer.capture_raw() {
                    Ok(raw_frame) => {
                        // Force keyframe every 5 seconds or on poor network conditions
                        let force_keyframe = frame_count % 150 == 0;
                        
                        if frame_count == 0 {
                            info!("Encoding first frame ({}x{} pixels, {} bytes)", 
                                  width, height, raw_frame.len());
                        }
                        
                        // Encode the frame
                        match encoder.encode_frame(&raw_frame, force_keyframe) {
                            Ok(encoded) => {
                                _frames_processed += 1;
                                
                                if frame_count == 0 {
                                    info!("Successfully encoded first frame ({} bytes)", encoded.len());
                                } else if frame_count % 300 == 0 { // Log every 10 seconds
                                    info!("Encoded frame {} ({} bytes)", frame_count, encoded.len());
                                }
                                
                                // Send encoded frame as video_frame message
                                if let Err(e) = encoded_tx.blocking_send(Ok(encoded)) {
                                    info!("Encoded frame channel closed, stopping screen capture thread: {}", e);
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
                                            error!("Software encoder also failed: {}", e);
                                            let _ = encoded_tx.blocking_send(Err(format!("All encoders failed: {}", e)));
                                            break;
                                        }
                                    }
                                } else {
                                    // Other encoding errors
                                    error!("Encoding failed: {}", e);
                                    let _ = encoded_tx.blocking_send(Err(format!("Encoding failed: {}", e)));
                                    break;
                                }
                            }
                        }
                        
                        frame_count += 1;
                    },
                    Err(e) => {
                        let err_msg = format!("Failed to capture raw frame: {}", e);
                        error!("{}", err_msg);
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
            } else {
                // No encoder available, cannot proceed
                error!("No video encoder available - capture loop cannot start");
                let _ = encoded_tx.blocking_send(Err("No video encoder available".to_string()));
                break;
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
        "type": "server_info",
        "width": width,
        "height": height,
        "hostname": hostname,
        "monitor": selected_monitor,
        "monitor_index": monitor_index,
        "codec": "vp8",  // Always send VP8 since that's our only supported codec
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
                                "switch_codec" => {
                                    // Handle codec switch request
                                    if let Some(new_codec) = value.get("codec").and_then(|c| c.as_str()) {
                                        info!("Client requesting codec switch to: {}", new_codec);
                                        // For now, we'll just log this - actual codec switching would require
                                        // restarting the encoder with new settings
                                        let response = serde_json::json!({
                                            "type": "codec_switch_response",
                                            "success": false,
                                            "message": "Codec switching not yet implemented - please refresh page to change codec"
                                        });
                                        
                                        if let Err(e) = message_tx_clone.send(response.to_string()).await {
                                            error!("Failed to send codec switch response: {}", e);
                                        }
                                    }
                                },
                                "request_keyframe" => {
                                    // Handle keyframe request
                                    info!("Client requesting keyframe");
                                    
                                    // Send a keyframe request to the encoder via a channel
                                    // This will be handled by forcing a keyframe on the next encode
                                    let keyframe_request = serde_json::json!({
                                        "type": "force_keyframe"
                                    });
                                    
                                    // For now, just acknowledge the request - the encoder will handle keyframes
                                    let response = serde_json::json!({
                                        "type": "keyframe_response",
                                        "status": "requested",
                                        "timestamp": SystemTime::now()
                                            .duration_since(SystemTime::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis()
                                    });
                                    
                                    if let Err(e) = message_tx_clone.send(response.to_string()).await {
                                        error!("Failed to send keyframe response: {}", e);
                                    }
                                    
                                    // Log that we're processing the keyframe request
                                    info!("Keyframe request acknowledged, will force keyframe on next capture");
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
        
        // Handle VP8 codec
        if codec == "vp8" {
            // Process VP8 encoded frames
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
                    
                    if frame_count == 0 {
                        info!("Sending first video frame to client ({} bytes encoded)", encoded_data.len());
                    } else if frame_count % 300 == 0 { // Log every 10 seconds
                        info!("Sent frame {} to client", frame_count);
                    }
                    
                    // Send the frame
                    if let Err(e) = message_tx_for_frames.send(frame_msg.to_string()).await {
                        error!("Failed to send encoded frame: {}", e);
                        break;
                    }
                    
                    frame_count += 1;
                },
                Some(Err(e)) => {
                    error!("Encoder error received: {}", e);
                    
                    // Send error message to client
                    let error_msg = serde_json::json!({
                        "type": "error",
                        "message": format!("Video encoding error: {}", e)
                    });
                    
                    if let Err(send_err) = message_tx_for_frames.send(error_msg.to_string()).await {
                        error!("Failed to send error message: {}", send_err);
                    }
                    break;
                },
                None => {
                    error!("Encoder channel closed unexpectedly");
                    
                    // Send error message to client
                    let error_msg = serde_json::json!({
                        "type": "error",
                        "message": "Video encoder stopped unexpectedly"
                    });
                    
                    if let Err(send_err) = message_tx_for_frames.send(error_msg.to_string()).await {
                        error!("Failed to send error message: {}", send_err);
                    }
                    break;
                }
            }
        } else {
            // No valid codec selected
            error!("Invalid codec: {}", codec);
            break;
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
    
    // Wait for screen capture thread to finish with timeout
    let join_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || screen_handle.join())
    ).await;
    
    match join_result {
        Ok(Ok(Ok(()))) => {
            info!("Screen capture thread joined successfully");
        },
        Ok(Ok(Err(e))) => {
            error!("Screen capture thread panicked: {:?}", e);
        },
        Ok(Err(e)) => {
            error!("Failed to spawn blocking task for thread join: {:?}", e);
        },
        Err(_) => {
            warn!("Timeout waiting for screen capture thread to join - thread may still be running");
        }
    }
}

// New handler with stop signal
pub async fn handle_socket_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    codec: String, 
    enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    info!("New WebSocket connection with stop signal: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Use WebRTC streaming for optimal performance
    if codec == "webrtc" {
        handle_webrtc_socket_with_stop(socket, monitor, enable_audio, stop_rx).await;
    } else {
        // Fallback to legacy for other codecs
        handle_legacy_socket_with_stop(socket, monitor, "vp8".to_string(), enable_audio, stop_rx).await;
    }
}

async fn handle_legacy_socket_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    _codec: String, 
    enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    let codec = "vp8".to_string(); // Always use VP8
    info!("New WebSocket connection: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Create a thread-safe channel for screen data
    let (screen_tx, mut screen_rx) = mpsc::channel::<Result<Vec<u8>, String>>(10);
    let (encoded_tx, mut encoded_rx) = mpsc::channel::<Result<Vec<u8>, String>>(10);

    // Create a control channel for codec switching and other commands
    let (control_tx, mut control_rx) = mpsc::channel::<ControlMessage>(10);

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
    let initial_codec = codec.clone();
    let screen_handle = std::thread::spawn(move || {
        info!("Starting screen capture thread for monitor {} with codec {}", monitor_index, initial_codec);
        let mut screen_capturer = match ScreenCapture::new(Some(monitor_index)) {
            Ok(capturer) => {
                info!("Screen capturer initialized successfully");
                capturer
            },
            Err(e) => {
                let err_msg = format!("Failed to initialize screen capture: {}", e);
                error!("{}", err_msg);
                let _ = screen_tx.blocking_send(Err(err_msg.clone()));
                let _ = encoded_tx.blocking_send(Err(err_msg));
                return;
            }
        };

        info!("Screen capturer dimensions: {}x{}", screen_capturer.dimensions().0, screen_capturer.dimensions().1);

        let (width, height) = screen_capturer.dimensions();
        let (tile_width, tile_height, tile_size) = screen_capturer.tile_dimensions();
        let dimensions = (width, height, tile_width, tile_height, tile_size);
        let _ = screen_tx.blocking_send(Ok(bincode::serialize(&dimensions).unwrap_or_default()));

        let current_quality = super::models::DEFAULT_QUALITY;

        // Codec state
        let mut current_codec = initial_codec.clone();
        let mut codec_type = CodecType::from_string(&current_codec);
        let mut video_encoder: Option<VideoEncoder> = None;
        
        let encoder_config = EncoderConfig {
            width: width as u32,
            height: height as u32,
            bitrate: 2_000_000,
            framerate: 30,
            keyframe_interval: 60,
            preset: "ultrafast".to_string(),
            use_hardware: false,
            codec_type: codec_type.clone(),
        };
        match VideoEncoder::new(encoder_config) {
            Ok(encoder) => {
                info!("Successfully initialized {:?} encoder for {}x{}", codec_type, width, height);
                video_encoder = Some(encoder);
            },
            Err(e) => {
                error!("Encoder initialization failed: {}", e);
                // Without encoder, cannot proceed
                return;
            }
        }

        // Control message handling (codec switch)
        let (codec_switch_tx, codec_switch_rx) = std::sync::mpsc::channel::<String>();
        let mut control_rx_for_thread = control_rx;
        // Spawn a thread to listen for control messages (codec switching disabled)
        let codec_switch_tx_clone = codec_switch_tx.clone();
        std::thread::spawn(move || {
            while let Some(control_msg) = control_rx_for_thread.blocking_recv() {
                if let ControlMessage::SwitchCodec { codec: _ } = control_msg {
                    info!("[Screen Thread] Codec switch requested but VP8 is enforced");
                    // Always use VP8 - ignore codec switch requests
                    let _ = codec_switch_tx_clone.send("vp8".to_string());
                }
            }
        });

        let mut frame_count = 0u64;
        let mut _frames_processed = 0u64;

        info!("Starting screen capture loop...");

        loop {
            // Check for stop signal (non-blocking) - check multiple times to ensure we catch it
            match thread_stop_rx.try_recv() {
                Ok(_) => {
                    info!("Screen capture thread received stop signal, exiting");
                    break;
                },
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    info!("Screen capture thread stop channel disconnected, exiting");
                    break;
                },
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No stop signal yet, continue
                }
            }

            // Check for codec switch (non-blocking)
            if let Ok(new_codec) = codec_switch_rx.try_recv() {
                if new_codec != current_codec {
                    info!("Switching encoder from {} to {}", current_codec, new_codec);
                    current_codec = new_codec.clone();
                    codec_type = CodecType::from_string(&current_codec);
                    // Drop encoder if switching codecs
                    if current_codec != "vp8" {
                        video_encoder = None;
                    } else {
                        // Re-initialize encoder for new codec
                        let encoder_config = EncoderConfig {
                            width: width as u32,
                            height: height as u32,
                            bitrate: 2_000_000,
                            framerate: 30,
                            keyframe_interval: 60,
                            preset: "ultrafast".to_string(),
                            use_hardware: false,
                            codec_type: codec_type.clone(),
                        };
                        match VideoEncoder::new(encoder_config) {
                            Ok(encoder) => {
                                info!("Re-initialized encoder for codec {:?}", codec_type);
                                video_encoder = Some(encoder);
                            },
                            Err(e) => {
                                error!("Failed to re-initialize encoder: {}", e);
                                break;
                            }
                        }
                    }
                }
            }

            frame_count += 1;

            if let Some(encoder) = &mut video_encoder {
                let _capture_start = Instant::now();
                debug!("Attempting to capture frame {}", frame_count);
                match screen_capturer.capture_raw() {
                    Ok(raw_frame) => {
                        debug!("Successfully captured frame {} ({} bytes)", frame_count, raw_frame.len());
                        let force_keyframe = frame_count % 150 == 0;
                        if force_keyframe {
                            debug!("Forcing keyframe for frame {}", frame_count);
                        }
                        match encoder.encode_frame(&raw_frame, force_keyframe) {
                            Ok(encoded) => {
                                _frames_processed += 1;
                                debug!("Successfully encoded frame {} ({} bytes)", frame_count, encoded.len());
                                if let Err(_) = encoded_tx.blocking_send(Ok(encoded)) {
                                    info!("Encoded frame channel closed, stopping screen capture thread");
                                    break;
                                }
                            },
                            Err(e) => {
                                if e.to_string().contains("Hardware encoder failed to open") {
                                    error!("Hardware encoder failed ({}), switching to software encoder", e);
                                    let software_config = EncoderConfig {
                                        width: width as u32,
                                        height: height as u32,
                                        bitrate: 2_000_000,
                                        framerate: 30,
                                        keyframe_interval: 60,
                                        preset: "ultrafast".to_string(),
                                        use_hardware: false,
                                        codec_type: codec_type.clone(),
                                    };
                                    match VideoEncoder::new(software_config) {
                                        Ok(new_encoder) => {
                                            info!("Successfully switched to software encoder");
                                            *encoder = new_encoder;
                                            continue;
                                        },
                                        Err(e) => {
                                            error!("Software encoder also failed: {}, stopping capture", e);
                                            break;
                                        }
                                    }
                                } else {
                                    warn!("Encoding failed: {}", e);
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
                // No encoder available
                warn!("Video encoder not available for frame {}", frame_count);
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            // Check for stop signal again before sleeping
            if thread_stop_rx.try_recv().is_ok() {
                info!("Screen capture thread received stop signal during sleep, exiting");
                break;
            }

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
                    // Try to parse as control message first
                    if let Ok(control_msg) = serde_json::from_str::<ControlMessage>(&text) {
                        debug!("Received control message: {:?}", control_msg);
                        // Forward control message to screen thread
                        if let Err(e) = control_tx.send(control_msg).await {
                            error!("Failed to forward control message to screen thread: {}", e);
                        }
                    }
                    // If not a control message, try to parse as input event
                    else if let Ok(event) = serde_json::from_str::<InputEvent>(&text) {
                        if let Err(e) = input_tx_for_messages.send(event).await {
                            error!("Failed to send input event: {}", e);
                            break;
                        }
                    }
                    else {
                        debug!("Failed to parse message as control or input event - text: {}", text);
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
                
                // Handle encoded frames (H.264)
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
    info!("Sending stop signal to screen capture thread");
    let _ = thread_stop_tx.try_send(());
    
    // Give the thread a moment to process the stop signal
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Wait for screen capture thread to finish with timeout
    let join_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || screen_handle.join())
    ).await;
    
    match join_result {
        Ok(Ok(Ok(()))) => {
            info!("Screen capture thread joined successfully");
        },
        Ok(Ok(Err(e))) => {
            error!("Screen capture thread panicked: {:?}", e);
        },
        Ok(Err(e)) => {
            error!("Failed to spawn blocking task for thread join: {:?}", e);
        },
        Err(_) => {
            warn!("Timeout waiting for screen capture thread to join - thread may still be running");
        }
    }
    
    info!("WebSocket connection with stop signal finished cleanup");
}

async fn handle_webrtc_socket(socket: WebSocket, monitor: usize, enable_audio: bool) {
    info!("Starting WebRTC H.264 streaming session: monitor={}, audio={}", monitor, enable_audio);
    
    // Create channels for frame data and control
    let (frame_tx, mut frame_rx) = mpsc::channel::<EncodedFrameMessage>(10);
    let (control_tx, mut control_rx) = mpsc::channel::<StreamingControl>(10);
    
    // Initialize screen capture first to get dimensions
    let (screen_width, screen_height) = match ScreenCapture::new(Some(monitor)) {
        Ok(capture) => (capture.width(), capture.height()),
        Err(e) => {
            error!("Failed to initialize screen capture: {}", e);
            return;
        }
    };
    
    // Split the WebSocket for sending and receiving
    let (mut sender, mut receiver) = socket.split();
    
    // Create channels for communication
    let (client_tx, mut client_rx) = mpsc::channel::<Message>(10);
    let client_tx_clone = client_tx.clone();
    let client_tx_clone2 = client_tx.clone();
    let client_tx_clone3 = client_tx.clone();
    
    // Send initial server info
    let server_info = json!({
        "type": "server_info",
        "width": screen_width,
        "height": screen_height,
        "hostname": gethostname::gethostname().to_string_lossy(),
        "codec": "webrtc",
        "audio": enable_audio,
        "monitor": monitor
    });
    
    if let Err(e) = client_tx_clone3.send(Message::Text(server_info.to_string())).await {
        error!("Failed to send server info: {}", e);
        return;
    }
    
    // Start capture and encoding task
    let frame_tx_clone = frame_tx.clone();
    let capture_task = tokio::task::spawn_blocking(move || {
        // Create capture and encoder in the worker thread
        let mut capture = match ScreenCapture::new(Some(monitor)) {
            Ok(capture) => capture,
            Err(e) => {
                error!("Failed to initialize screen capture in worker thread: {}", e);
                return;
            }
        };
        
        let encoder_config = EncoderConfig {
            width: screen_width as u32,
            height: screen_height as u32,
            bitrate: 2_000_000, // 2 Mbps default
            framerate: 30,
            keyframe_interval: 30,
            preset: "ultrafast".to_string(),
            use_hardware: false, // Start with software, fallback is built-in
            codec_type: CodecType::VP8,
        };
        
        let mut encoder = match VideoEncoder::new(encoder_config.clone()) {
            Ok(encoder) => encoder,
            Err(e) => {
                error!("Failed to initialize video encoder in worker thread: {}", e);
                return;
            }
        };
        
        let rt = tokio::runtime::Handle::current();
        let mut frame_count = 0u32;
        let mut last_keyframe = std::time::Instant::now();
        let keyframe_interval = std::time::Duration::from_secs(2);
        
        loop {
            // Capture screen
            let frame_data = match capture.capture_rgba() {
                Ok(data) => data,
                Err(e) => {
                    error!("Screen capture failed: {}", e);
                    std::thread::sleep(std::time::Duration::from_millis(33)); // ~30 FPS
                    continue;
                }
            };
            
            // Force keyframe every 2 seconds
            let force_keyframe = last_keyframe.elapsed() > keyframe_interval;
            if force_keyframe {
                last_keyframe = std::time::Instant::now();
            }
            
            // Encode frame
            let encoded_data = match encoder.encode_frame(&frame_data, force_keyframe) {
                Ok(data) => data,
                Err(e) => {
                    error!("Frame encoding failed: {}", e);
                    std::thread::sleep(std::time::Duration::from_millis(33));
                    continue;
                }
            };
            
            if !encoded_data.is_empty() {
                let frame_msg = EncodedFrameMessage {
                    data: encoded_data,
                    is_keyframe: force_keyframe,
                    timestamp: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                    sequence_number: frame_count,
                    rtp_packets: vec![], // Not used for WebSocket streaming
                };
                
                if let Err(_) = rt.block_on(frame_tx_clone.send(frame_msg)) {
                    warn!("Frame channel closed, stopping capture");
                    break;
                }
                
                frame_count = frame_count.wrapping_add(1);
            }
            
            // Control frame rate (~30 FPS)
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
    });
    
    // Handle incoming WebSocket messages
    let control_tx_clone = control_tx.clone();
    let client_message_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<ControlMessage>(&text) {
                        Ok(ControlMessage::NetworkStats { latency, bandwidth, packet_loss }) => {
                            if let (Some(latency), Some(bandwidth), Some(packet_loss)) = (latency, bandwidth, packet_loss) {
                                let stats = NetworkStats { latency, bandwidth, packet_loss };
                                if let Err(e) = control_tx_clone.send(StreamingControl::NetworkStatsUpdate(stats)).await {
                                    error!("Failed to send network stats: {}", e);
                                }
                            }
                        },
                        Ok(ControlMessage::RequestKeyframe) => {
                            if let Err(e) = control_tx_clone.send(StreamingControl::RequestKeyframe).await {
                                error!("Failed to request keyframe: {}", e);
                            }
                        },
                        Ok(ControlMessage::QualitySetting { quality }) => {
                            let quality_profile = match quality {
                                q if q >= 80 => QualityProfile::High,
                                q if q >= 50 => QualityProfile::Medium,
                                _ => QualityProfile::Low,
                            };
                            if let Err(e) = control_tx_clone.send(StreamingControl::UpdateQuality(quality_profile)).await {
                                error!("Failed to update quality: {}", e);
                            }
                        },
                        Ok(ControlMessage::Ping { timestamp }) => {
                            let response = json!({
                                "type": "pong",
                                "timestamp": timestamp
                            });
                            if let Err(e) = client_tx_clone.send(Message::Text(response.to_string())).await {
                                error!("Failed to send pong: {}", e);
                            }
                        },
                        Ok(ControlMessage::SwitchCodec { codec: _ }) => {
                            // VP8 is enforced - codec switching disabled
                            warn!("Codec switching disabled - VP8 is enforced");
                        },
                        Err(e) => {
                            error!("Failed to parse control message: {}", e);
                        }
                    }
                },
                Ok(Message::Close(_)) => {
                    info!("WebSocket closed by client");
                    break;
                },
                Ok(_) => {
                    // Ignore other message types
                },
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }
    });
    
    // Handle outgoing frame data
    let frame_sender_task = tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            let frame_message = json!({
                "type": "webrtc_frame",
                "data": general_purpose::STANDARD.encode(&frame.data),
                "is_keyframe": frame.is_keyframe,
                "timestamp": frame.timestamp,
                "sequence_number": frame.sequence_number
            });
            
            if let Err(e) = client_tx_clone2.send(Message::Text(frame_message.to_string())).await {
                error!("Failed to send frame: {}", e);
                break;
            }
        }
    });
    
    // Handle client messages
    let client_handler_task = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
            if let Err(e) = sender.send(msg).await {
                error!("Failed to send message to client: {}", e);
                break;
            }
        }
    });
    
    // Send initial server info
    let server_info = json!({
        "type": "server_info",
        "monitor": monitor,
        "codec": "vp8",  // Always send VP8 since that's our only supported codec
        "audio": enable_audio,
        "width": 1920, // TODO: Get actual screen dimensions
        "height": 1080,
        "hostname": "localhost"
    });
    
    let _ = client_tx_clone3.send(Message::Text(server_info.to_string())).await;
    
    // Send monitor list
    let monitor_list = json!({
        "type": "monitors",
        "monitors": [
            {
                "id": "primary",
                "name": "Primary Monitor",
                "width": 1920,
                "height": 1080,
                "position_x": 0,
                "position_y": 0,
                "is_primary": true,
                "scale_factor": 1.0
            }
        ]
    });
    
    let _ = client_tx_clone3.send(Message::Text(monitor_list.to_string())).await;
    
    // Wait for any task to complete
    tokio::select! {
        _ = client_message_task => {
            info!("Client message task completed");
        }
        _ = frame_sender_task => {
            info!("Frame sender task completed");
        }
        _ = client_handler_task => {
            info!("Client handler task completed");
        }
        _ = capture_task => {
            info!("Capture task completed");
        }
    }
    
    // Send stop signal
    if let Err(e) = control_tx.send(StreamingControl::Stop).await {
        error!("Failed to send stop signal: {}", e);
    }
    
    info!("WebRTC socket handler completed");
}

async fn handle_webrtc_socket_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    enable_audio: bool, 
    mut stop_rx: broadcast::Receiver<()>
) {
    info!("Starting WebRTC H.264 streaming session with stop signal: monitor={}, audio={}", monitor, enable_audio);
    
    tokio::select! {
        _ = handle_webrtc_socket(socket, monitor, enable_audio) => {
            info!("WebRTC socket handler completed");
        }
        _ = stop_rx.recv() => {
            info!("WebRTC socket handler stopped by signal");
        }
    }
}
