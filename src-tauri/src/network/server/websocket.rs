use crate::streaming::{RealtimeConfig};
use crate::streaming::RealtimeStreamHandler;
use crate::streaming::UltraStreamHandler; // Ultra-low latency handler
use axum::extract::ws::WebSocket;
use tokio::{sync::broadcast};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};

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

// Helper function to make the future Send - now uses real-time streaming
pub async fn handle_socket_wrapper(socket: WebSocket, monitor: usize, _codec: String, enable_audio: bool) {
    // Always use VP8 codec with real-time streaming
    info!("New real-time streaming WebSocket connection - Monitor: {}, Codec: VP8, Audio: {}", 
          monitor, enable_audio);
    
    handle_realtime_socket(socket, monitor, enable_audio, None).await;
    
    info!("Real-time streaming WebSocket connection closed - Monitor: {}", monitor);
}

// New helper function with stop signal - uses real-time streaming
pub async fn handle_socket_wrapper_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    _codec: String, 
    enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    info!("New real-time streaming WebSocket connection with stop signal - Monitor: {}, Codec: VP8, Audio: {}", 
          monitor, enable_audio);
    
    handle_realtime_socket(socket, monitor, enable_audio, Some(stop_rx)).await;
    
    info!("Real-time streaming WebSocket connection with stop signal closed - Monitor: {}", monitor);
}

pub async fn handle_socket_with_stop(
    socket: WebSocket, 
    monitor: usize, 
    codec: String, 
    enable_audio: bool, 
    stop_rx: broadcast::Receiver<()>
) {
    info!("New WebSocket connection with stop signal: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Always use real-time streaming for connections with stop signal
    handle_realtime_socket(socket, monitor, enable_audio, Some(stop_rx)).await;
}

pub async fn handle_socket(socket: WebSocket, monitor: usize, codec: String, enable_audio: bool) {
    info!("New WebSocket connection: monitor={}, codec={}, audio={}", 
          monitor, codec, enable_audio);
    
    // Always use real-time streaming for direct connections
    handle_realtime_socket(socket, monitor, enable_audio, None).await;
}

// New ultra-low latency streaming socket handler with Google/Microsoft optimizations
async fn handle_realtime_socket(
    socket: WebSocket, 
    monitor: usize, 
    _enable_audio: bool, // Audio support can be added later
    stop_rx: Option<broadcast::Receiver<()>>
) {
    info!("🚀 Starting ULTRA-LOW LATENCY streaming session for monitor {}", monitor);
    
    // Use ultra-high performance handler for <16ms total latency
    match UltraStreamHandler::new(monitor) {
        Ok(handler) => {
            info!("✅ Ultra-low latency handler initialized successfully");
            handler.handle_connection(socket, stop_rx).await;
        }
        Err(e) => {
            error!("❌ Failed to create ultra-low latency handler: {}", e);
            
            // Fallback to standard real-time streaming with IMPROVED QUALITY
            warn!("🔄 Falling back to standard real-time streaming");
            
            let config = RealtimeConfig {
                monitor_id: monitor,
                width: 1920,
                height: 1080,  
                bitrate: 12000, // MUCH higher bitrate for excellent quality
                framerate: 60,  // Smooth 60fps
                keyframe_interval: 60, // More frequent keyframes (every 1 second)
                target_latency_ms: 100, // Relaxed latency for better quality
            };
            
            match RealtimeStreamHandler::new(config) {
                Ok(fallback_handler) => {
                    fallback_handler.handle_connection(socket, stop_rx).await;
                }
                Err(fallback_error) => {
                    error!("💥 Failed to create fallback handler: {}", fallback_error);
                }
            }
        }
    }
}

pub async fn handle_socket_ultra(
    ws: axum::extract::WebSocketUpgrade,
    query: axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl axum::response::IntoResponse {
    let monitor = query.get("monitor")
        .and_then(|m| m.parse::<usize>().ok())
        .unwrap_or(0);

    info!("🔌 Ultra WebSocket connection request for monitor {}", monitor);

    ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_ultra_connection(socket, monitor).await {
            error!("❌ Ultra WebSocket connection failed: {}", e);
        }
    })
}

async fn handle_ultra_connection(socket: WebSocket, monitor: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("⚡ Starting ultra-performance streaming for monitor {}", monitor);
    
    // Try ultra-performance first, graceful fallback to standard
    match crate::streaming::UltraStreamHandler::new(monitor) {
        Ok(ultra_handler) => {
            info!("🚀 Using ULTRA-PERFORMANCE streaming mode");
            ultra_handler.handle_connection(socket, Some(tokio::sync::broadcast::channel(1).1)).await;
        },
        Err(e) => {
            warn!("⚠️  Ultra-performance mode failed: {} - falling back to standard mode", e);
            
            // Fallback to standard real-time streaming with HIGH QUALITY
            let config = crate::streaming::RealtimeConfig {
                monitor_id: monitor,
                width: 1920,
                height: 1080,  
                bitrate: 15000, // Very high bitrate for excellent quality
                framerate: 60,  // Smooth standard framerate
                keyframe_interval: 60, // Frequent keyframes for stability
                target_latency_ms: 150, // Generous latency budget for quality
            };
            
            match crate::streaming::RealtimeStreamHandler::new(config) {
                Ok(fallback_handler) => {
                    info!("🔄 Using STANDARD real-time streaming mode");
                    fallback_handler.handle_connection(socket, Some(tokio::sync::broadcast::channel(1).1)).await;
                },
                Err(e) => {
                    error!("❌ Both ultra and standard streaming failed: {}", e);
                    return Err(e.into());
                }
            }
        }
    }
    
    Ok(())
}
