use crate::streaming::{
    RealtimeConfig, 
    IntegratedStreamHandler, 
    IntegratedStreamConfig,
    RealtimeStreamHandler,
    UltraStreamHandler,
    // EnhancedVideoEncoder,
    // EnhancedAudioEncoder
};
use axum::extract::{ws::{WebSocket, WebSocketUpgrade, Message}, Query};
use axum::response::IntoResponse;
use std::collections::HashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
    
    #[serde(rename = "bitrate_setting")]
    BitrateSetting { bitrate: u32 },
    
    #[serde(rename = "webm_config")]
    WebMConfig { 
        enable_vp8: bool,
        enable_opus: bool,
        target_bitrate: Option<u32>
    },
    
    #[serde(rename = "network_stats")]
    NetworkStats { 
        latency: Option<u32>,
        bandwidth: Option<f32>,
        packet_loss: Option<f32>
    },
}

// Helper function to make the future Send - now uses integrated YUV420 + WebM streaming
pub async fn handle_socket_wrapper(socket: WebSocket, monitor: usize, codec: String, enable_audio: bool) {
    info!("🎬 New YUV420 + WebM streaming WebSocket connection - Monitor: {}, Codec: {}, Audio: {}", 
          monitor, codec, enable_audio);
    
    handle_integrated_webm_socket(socket, monitor, enable_audio, None).await;
    
    info!("✅ YUV420 + WebM streaming WebSocket connection closed - Monitor: {}", monitor);
}

// New helper function with stop signal - uses integrated WebM streaming
pub async fn handle_socket_wrapper_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    codec: String, 
    enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    info!("🎬 New YUV420 + WebM streaming WebSocket connection with stop signal - Monitor: {}, Codec: {}, Audio: {}", 
          monitor, codec, enable_audio);
    
    handle_integrated_webm_socket(socket, monitor, enable_audio, Some(stop_rx)).await;
    
    info!("✅ YUV420 + WebM streaming WebSocket connection with stop signal closed - Monitor: {}", monitor);
}

pub async fn handle_socket_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    codec: String, 
    enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    info!("🎬 New WebSocket connection with stop signal: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Always use integrated WebM streaming for connections with stop signal
    handle_integrated_webm_socket(socket, monitor, enable_audio, Some(stop_rx)).await;
}

pub async fn handle_socket(socket: WebSocket, monitor: usize, codec: String, enable_audio: bool) {
    info!("🎬 New WebSocket connection: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Always use integrated WebM streaming for direct connections
    handle_integrated_webm_socket(socket, monitor, enable_audio, None).await;
}

// New integrated YUV420 + WebM streaming socket handler
async fn handle_integrated_webm_socket(
    socket: WebSocket, 
    monitor: usize, 
    enable_audio: bool,
    stop_rx: Option<broadcast::Receiver<()>>
) {
    info!("🚀 WebM streaming requested for monitor {} - falling back to RGBA streaming for now", monitor);
    
    // TODO: Implement proper WebM/VP8 encoding
    // For now, fall back to working RGBA streaming
    info!("🔄 Using RGBA streaming until WebM/VP8 encoding is implemented");
    
    // Create handler before async context to avoid Send issues on macOS
    let handler_result = UltraStreamHandler::new(monitor);
    
    match handler_result {
        Ok(handler) => {
            info!("✅ RGBA streaming handler initialized successfully");
            handler.handle_connection(socket, stop_rx).await;
        }
        Err(e) => {
            error!("❌ Failed to create RGBA streaming handler: {}", e);
            
            // Final fallback to standard real-time streaming with enhanced quality
            info!("🔄 Final fallback to enhanced real-time streaming...");
            let enhanced_config = RealtimeConfig {
                monitor_id: monitor,
                width: 1920,
                height: 1080,  
                bitrate: 8000, // High bitrate for quality
                framerate: 30,  // Stable framerate
                keyframe_interval: 30, // Frequent keyframes
                target_latency_ms: 150, // Balanced latency
            };
            
            match RealtimeStreamHandler::new(enhanced_config) {
                Ok(handler) => {
                    info!("✅ Enhanced real-time fallback handler initialized");
                    handler.handle_connection(socket, stop_rx).await;
                }
                Err(e) => {
                    error!("❌ All streaming handlers failed to initialize: {}", e);
                }
            }
        }
    }
}

pub async fn handle_socket_ultra(
    ws: WebSocketUpgrade, 
    Query(params): Query<HashMap<String, String>>
) -> impl IntoResponse {
    let monitor = params.get("monitor").map(|v| v.parse::<usize>().unwrap_or(0)).unwrap_or(0);

    info!("🔌 Ultra WebSocket connection request for monitor {}", monitor);

    // Platform-specific handling for macOS Send trait issues
    #[cfg(target_os = "macos")]
    {
        ws.on_upgrade(move |socket| async move {
            info!("🍎 Using macOS ultra fallback for monitor {}", monitor);
            if let Err(e) = handle_macos_ultra_fallback(socket, monitor).await {
                error!("❌ macOS Ultra WebSocket connection failed: {}", e);
            }
        })
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        ws.on_upgrade(move |socket| async move {
            if let Err(e) = handle_ultra_connection(socket, monitor).await {
                error!("❌ Ultra WebSocket connection failed: {}", e);
            }
        })
    }
}

#[cfg(target_os = "macos")]
async fn handle_macos_ultra_fallback(socket: WebSocket, monitor: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("⚡ Starting macOS ultra-performance fallback streaming for monitor {}", monitor);
    
    // Use the same fallback logic as in handlers.rs
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};
    use xcap::Monitor;
    
    let (mut sender, mut receiver) = socket.split();
    
    // Send initial info
    let _ = sender.send(Message::Text(serde_json::json!({
        "type": "server_info",
        "version": "3.0.0-macos-ultra-fallback",
        "platform": "macOS",
        "streaming_mode": "ultra-fallback"
    }).to_string())).await;
    
    // Simple high-performance capture loop
    let capture_task = tokio::spawn(async move {
        let mut frame_count = 0u64;
        
        loop {
            let monitors = match Monitor::all() {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to get monitors: {:?}", e);
                    break;
                }
            };
            
            let monitor_obj = match monitors.get(monitor) {
                Some(m) => m,
                None => {
                    error!("Monitor {} not found", monitor);
                    break;
                }
            };
            
            match monitor_obj.capture_image() {
                Ok(image) => {
                    let mut frame_data = Vec::with_capacity(image.as_raw().len() + 24);
                    frame_data.extend_from_slice(b"RGBA");
                    frame_data.extend_from_slice(&(image.width() as u32).to_le_bytes());
                    frame_data.extend_from_slice(&(image.height() as u32).to_le_bytes());
                    frame_data.extend_from_slice(&frame_count.to_le_bytes());
                    frame_data.extend_from_slice(&(image.as_raw().len() as u32).to_le_bytes());
                    frame_data.extend_from_slice(image.as_raw());
                    
                    if let Err(_) = sender.send(Message::Binary(frame_data)).await {
                        break;
                    }
                    
                    frame_count += 1;
                },
                Err(e) => {
                    error!("Ultra capture failed: {:?}", e);
                }
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(16)).await; // ~60 FPS for ultra mode
        }
        
        info!("🍎 macOS ultra fallback capture ended");
    });
    
    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Close(_)) => {
                info!("Ultra WebSocket connection closed");
                break;
            },
            Err(e) => {
                error!("Ultra WebSocket error: {}", e);
                break;
            },
            _ => {}
        }
    }
    
    capture_task.abort();
    Ok(())
}

#[cfg(not(target_os = "macos"))]async fn handle_ultra_connection(socket: WebSocket, monitor: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("⚡ Starting ultra-performance YUV420 + WebM streaming for monitor {}", monitor);
    
    // Create handler before async context to avoid Send issues on macOS
    let handler_result = crate::streaming::UltraStreamHandler::new(monitor);
    
    // Try ultra-performance WebM streaming first
    match handler_result {
        Ok(ultra_handler) => {
            info!("🚀 Using ULTRA-PERFORMANCE YUV420 + WebM streaming mode");
            ultra_handler.handle_connection(socket, Some(tokio::sync::broadcast::channel(1).1)).await;
        },
        Err(e) => {
            warn!("⚠️  Ultra-performance WebM mode failed: {} - falling back to enhanced mode", e);
            
            // Fallback to enhanced real-time streaming with WebM support
            let enhanced_config = crate::streaming::RealtimeConfig {
                monitor_id: monitor,
                width: 1920,
                height: 1080,  
                bitrate: 10000, // Very high bitrate for excellent WebM quality
                framerate: 60,  // Smooth framerate
                keyframe_interval: 60, // Frequent keyframes for WebM stability
                target_latency_ms: 120, // Optimized latency for WebM
            };
            
            match crate::streaming::RealtimeStreamHandler::new(enhanced_config) {
                Ok(fallback_handler) => {
                    info!("🔄 Using ENHANCED WebM real-time streaming mode");
                    fallback_handler.handle_connection(socket, Some(tokio::sync::broadcast::channel(1).1)).await;
                },
                Err(e) => {
                    error!("❌ Both ultra and enhanced WebM streaming failed: {}", e);
                    return Err(e.into());
                }
            }
        }
    }
    
    Ok(())
}
