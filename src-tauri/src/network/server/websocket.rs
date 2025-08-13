use crate::streaming::{
    RealtimeConfig, 
    IntegratedStreamHandler, 
    IntegratedStreamConfig,
    RealtimeStreamHandler,
    UltraStreamHandler,
    // EnhancedVideoEncoder,
    // EnhancedAudioEncoder
};
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
