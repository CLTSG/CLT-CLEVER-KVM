use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::{sync::{broadcast, Mutex}, time};
use log::{debug, error, info, warn};
use serde_json::json;
use anyhow::Result;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use parking_lot::RwLock;

use crate::streaming::{UltraLowLatencyEncoder, UltraLowLatencyConfig, PerformanceTarget};
use crate::streaming::RealtimeStreamHandler; // Fallback handler
use crate::core::InputHandler;
use crate::network::models::NetworkStats;

/// Ultra-high performance streaming handler for <16ms total latency
/// Implements Google/Microsoft level optimizations for real-time streaming
/// With graceful fallback to standard realtime streaming when performance targets are not met
pub struct UltraStreamHandler {
    encoder: Arc<Mutex<UltraLowLatencyEncoder>>,
    fallback_handler: Arc<Mutex<Option<RealtimeStreamHandler>>>, // Fallback for when ultra-mode fails
    input_handler: InputHandler,
    
    // Ultra-performance metrics
    frame_count: AtomicU64,
    last_keyframe_time: RwLock<Instant>,
    network_stats: Arc<RwLock<NetworkStats>>,
    
    // Zero-latency frame management
    frame_send_time: AtomicU64, // Nanosecond timestamp for latency measurement
    emergency_mode: AtomicBool,
    fallback_mode: AtomicBool,   // Track if we're in fallback mode
    consecutive_failures: AtomicU64, // Count consecutive budget violations
    
    // Advanced quality adaptation
    performance_mode: Arc<RwLock<PerformanceMode>>,
}

#[derive(Clone, Debug)]
enum PerformanceMode {
    UltraLowLatency,  // <16ms - for local high-end systems
    Gaming,           // <8ms - for competitive gaming
    Balanced,         // <32ms - standard quality/performance balance
    Emergency,        // Maximum performance, minimum quality
}

impl PerformanceMode {
    fn get_target(&self) -> PerformanceTarget {
        match self {
            PerformanceMode::UltraLowLatency => PerformanceTarget::ultra_low_latency(),
            PerformanceMode::Gaming => PerformanceTarget::gaming(),
            PerformanceMode::Balanced => PerformanceTarget::balanced(),
            PerformanceMode::Emergency => PerformanceTarget {
                capture_budget_ms: 4.0,
                encode_budget_ms: 2.0,
                total_budget_ms: 6.0,
                target_fps: 60,
                max_frame_queue: 0,
            },
        }
    }
    
    fn get_interval_ms(&self) -> u64 {
        match self {
            PerformanceMode::UltraLowLatency => 8,  // 120 FPS
            PerformanceMode::Gaming => 7,           // 144 FPS
            PerformanceMode::Balanced => 16,        // 60 FPS
            PerformanceMode::Emergency => 33,       // 30 FPS
        }
    }
}

impl UltraStreamHandler {
    pub fn new(monitor_id: usize) -> Result<Self> {
        info!("🚀 Initializing ULTRA-LOW LATENCY streaming handler");
        
        // Automatically detect optimal performance mode based on system capabilities
        let performance_mode = Self::detect_optimal_performance_mode();
        info!("🎯 Selected performance mode: {:?}", performance_mode);
        
        let config = UltraLowLatencyConfig {
            monitor_id,
            width: 1920,
            height: 1080,
            performance_target: performance_mode.get_target(),
            use_hardware_acceleration: true,
            enable_simd_optimization: true,
            enable_parallel_processing: true,
            adaptive_quality: true,
            target_latency_ms: 1000,
        };
        
        let encoder = Arc::new(Mutex::new(UltraLowLatencyEncoder::new(config)?));
        let input_handler = InputHandler::new();
        
        Ok(Self {
            encoder,
            fallback_handler: Arc::new(Mutex::new(None)),
            input_handler,
            frame_count: AtomicU64::new(0),
            last_keyframe_time: RwLock::new(Instant::now()),
            network_stats: Arc::new(RwLock::new(NetworkStats::default())),
            frame_send_time: AtomicU64::new(0),
            emergency_mode: AtomicBool::new(false),
            fallback_mode: AtomicBool::new(false),
            consecutive_failures: AtomicU64::new(0),
            performance_mode: Arc::new(RwLock::new(performance_mode)),
        })
    }
    
    /// Detect optimal performance mode based on system capabilities
    fn detect_optimal_performance_mode() -> PerformanceMode {
        // TODO: Implement system capability detection
        // For now, default to ultra-low latency mode
        PerformanceMode::UltraLowLatency
    }
    
    pub async fn handle_connection(
        self,
        socket: WebSocket,
        stop_rx: Option<broadcast::Receiver<()>>,
    ) {
        info!("🔥 Starting ULTRA-LOW LATENCY streaming session");
        
        let (mut sender, mut receiver) = socket.split();
        
        // High-performance channels with minimal buffering
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(1); // Single-buffer for ultra-low latency
        let (control_tx, mut control_rx) = tokio::sync::mpsc::channel::<String>(5);
        
        // Send initial server info with ultra-performance specifications
        {
            let encoder = self.encoder.lock().await;
            let (width, height) = encoder.get_dimensions();
            let performance_mode_str = {
                let mode = self.performance_mode.read();
                format!("{:?}", *mode)
            };
            
            let server_info = json!({
                "type": "server_info",
                "width": width,
                "height": height,
                "hostname": "ultra-kvm-server",
                "monitor": 0,
                "codec": "ultra-vp8",
                "audio": false,
                "performance_mode": performance_mode_str,
                "target_fps": 120,
                "ultra_features": {
                    "simd_optimization": true,
                    "parallel_processing": true,
                    "adaptive_quality": true,
                    "zero_copy": true,
                    "emergency_mode": false
                }
            });
            
            if let Err(e) = control_tx.send(server_info.to_string()).await {
                error!("Failed to send ultra server info: {}", e);
                return;
            }
        }
        
        // ULTRA-HIGH PERFORMANCE STREAMING TASK
        let encoder_clone = Arc::clone(&self.encoder);
        let performance_mode_clone = Arc::clone(&self.performance_mode);
        let emergency_mode_flag = Arc::new(AtomicBool::new(false));
        let emergency_mode_clone = Arc::clone(&emergency_mode_flag);
        
        // Clone necessary fields before moving self
        let fallback_handler = Arc::clone(&self.fallback_handler);
        let fallback_mode = Arc::clone(&Arc::new(AtomicBool::new(false)));
        let consecutive_failures = Arc::new(AtomicU64::new(0));
        let performance_mode_clone3 = Arc::clone(&self.performance_mode);
        
        let streaming_task = {
            let tx = tx.clone();
            let performance_mode_clone2 = Arc::clone(&performance_mode_clone);
            tokio::spawn(async move {
                let mut frame_count = 0u64;
                let mut last_keyframe_time = Instant::now();
                let mut last_stats_time = Instant::now();
                let mut consecutive_budget_violations = 0u32;
                
                loop {
                    let interval_ms = {
                        let performance_mode = performance_mode_clone2.read();
                        performance_mode.get_interval_ms()
                    };
                    
                    let mut interval = time::interval(Duration::from_millis(interval_ms));
                    interval.tick().await;
                    
                    // Check if we should force a keyframe (every 1 second)
                    let force_keyframe = last_keyframe_time.elapsed() > Duration::from_secs(1);
                    
                    // ULTRA-FAST CAPTURE AND ENCODE
                    let capture_start = Instant::now();
                    let encoded_data = {
                        let encoder = encoder_clone.lock().await;
                        
                        match encoder.capture_and_encode_ultra_fast(force_keyframe) {
                            Ok(Some(data)) => {
                                consecutive_budget_violations = 0; // Reset on success
                                data
                            },
                            Ok(None) => continue, // No data to send
                            Err(e) => {
                                error!("🔴 Ultra-encode error: {}", e);
                                consecutive_budget_violations += 1;
                                
                                // Track consecutive failures for fallback decision
                                let current_failures = consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
                                
                                // Activate fallback mode immediately for compatibility
                                if current_failures >= 1 && !fallback_mode.load(Ordering::Relaxed) {
                                    warn!("🔄 SWITCHING TO FALLBACK REALTIME STREAMING - Ultra mode failing consistently");
                                    fallback_mode.store(true, Ordering::Relaxed);
                                    
                                    // Initialize fallback handler
                                    let fallback_config = crate::streaming::RealtimeConfig {
                                        monitor_id: 0,
                                        width: 1920,
                                        height: 1080,
                                        framerate: 30,      // Lower FPS for stability
                                        bitrate: 2000,      // 2 Mbps for stable streaming
                                        keyframe_interval: 90, // Every 3 seconds at 30fps
                                        target_latency_ms: 100, // Higher latency for stability
                                    };
                                    
                                    match crate::streaming::RealtimeStreamHandler::new(fallback_config) {
                                        Ok(fallback) => {
                                            let mut fallback_handler_guard = fallback_handler.lock().await;
                                            *fallback_handler_guard = Some(fallback);
                                            info!("✅ Fallback handler initialized - switching to stable streaming");
                                        },
                                        Err(e) => {
                                            error!("Failed to initialize fallback handler: {}", e);
                                        }
                                    }
                                }
                                
                                // If in fallback mode, use simplified capture
                                if fallback_mode.load(Ordering::Relaxed) {
                                    info!("📹 Using fallback streaming mode");
                                    // Use a simple fallback capture that doesn't require self
                                    match fallback_simple_capture().await {
                                        Ok(Some(data)) => data,
                                        Ok(None) => continue,
                                        Err(e) => {
                                            error!("Fallback capture failed: {}", e);
                                            continue;
                                        }
                                    }
                                } else {
                                    // Emergency performance mode activation
                                    if consecutive_budget_violations >= 3 && !emergency_mode_clone.load(Ordering::Relaxed) {
                                        warn!("🚨 ACTIVATING EMERGENCY PERFORMANCE MODE");
                                        emergency_mode_clone.store(true, Ordering::Relaxed);
                                        encoder.emergency_performance_mode();
                                        
                                        // Switch to emergency performance mode
                                        let mut mode = performance_mode_clone2.write();
                                        *mode = PerformanceMode::Emergency;
                                    }
                                    
                                    tokio::time::sleep(Duration::from_millis(8)).await; // Minimal backoff
                                    continue;
                                }
                            }
                        }
                    };
                    
                    // Reset failure counter on any successful operation
                    if !fallback_mode.load(Ordering::Relaxed) {
                        consecutive_failures.store(0, Ordering::Relaxed);
                    }
                    
                    let total_time = capture_start.elapsed();
                    
                    if force_keyframe {
                        last_keyframe_time = Instant::now();
                    }
                    
                    frame_count += 1;
                    
                    // ZERO-LATENCY FRAME TRANSMISSION
                    let send_start = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64;
                    
                    if let Err(_) = tx.send(encoded_data).await {
                        break; // Channel closed
                    }
                    
                    // Ultra-performance monitoring
                    if last_stats_time.elapsed() > Duration::from_secs(2) {
                        let encoder = encoder_clone.lock().await;
                        let (capture_ms, encode_ms, total_frames, dropped_frames, latency_ms, quality_level) = 
                            encoder.get_ultra_performance_stats();
                        
                        let current_fps = frame_count as f64 / last_stats_time.elapsed().as_secs_f64();
                        
                        info!("⚡ ULTRA-PERF: fps={:.1}, capture={:.1}ms, encode={:.1}ms, latency={}ms, quality={}%, drops={}", 
                              current_fps, capture_ms, encode_ms, latency_ms, quality_level, dropped_frames);
                        
                        // Auto-optimize performance mode based on results
                        if latency_ms < 8 && dropped_frames == 0 {
                            // Excellent performance - can use gaming mode
                            let mut mode = performance_mode_clone2.write();
                            if !matches!(*mode, PerformanceMode::Gaming) {
                                *mode = PerformanceMode::Gaming;
                                info!("🎮 Switching to GAMING mode (ultra-low latency achieved)");
                            }
                        } else if latency_ms > 32 || dropped_frames > total_frames / 10 {
                            // Poor performance - use balanced mode
                            let mut mode = performance_mode_clone2.write();
                            if !matches!(*mode, PerformanceMode::Balanced | PerformanceMode::Emergency) {
                                *mode = PerformanceMode::Balanced;
                                info!("⚖️  Switching to BALANCED mode (performance issues detected)");
                            }
                        }
                        
                        last_stats_time = Instant::now();
                        frame_count = 0; // Reset for next measurement period
                    }
                }
                
                info!("🏁 Ultra streaming task ended");
            })
        };
        
        // ZERO-LATENCY SEND TASK
        let send_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Binary video data with timestamp for latency measurement
                    Some(data) = rx.recv() => {
                        debug!("🔍 [SEND] Received frame data: {} bytes", data.len());
                        
                        // Check frame header for debugging
                        if data.len() >= 3 {
                            let header = &data[0..3];
                            debug!("🔍 [SEND] Frame header: [0x{:02X}, 0x{:02X}, 0x{:02X}]", 
                                   header[0], header[1], header[2]);
                        }
                        
                        // For fallback mode, send data directly without timestamp prefix
                        // as the fallback_simple_capture already provides compatible format
                        if let Err(e) = sender.send(Message::Binary(data.clone())).await {
                            error!("🔴 [SEND] Failed to send video data: {}", e);
                            break;
                        } else {
                            debug!("✅ [SEND] Successfully transmitted {} bytes", data.len());
                        }
                    }
                    // Control messages
                    Some(text) = control_rx.recv() => {
                        if let Err(e) = sender.send(Message::Text(text)).await {
                            error!("Failed to send ultra control message: {}", e);
                            break;
                        }
                    }
                    else => break,
                }
            }
        });
        
        // ULTRA-RESPONSIVE INPUT HANDLING
        let encoder_clone2 = Arc::clone(&self.encoder);
        let control_tx_clone = control_tx.clone();
        let network_stats_clone = Arc::clone(&self.network_stats);
        
        let receive_task = tokio::spawn(async move {
            while let Some(msg) = receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(json_msg) = serde_json::from_str::<serde_json::Value>(&text) {
                            match json_msg.get("type").and_then(|t| t.as_str()) {
                                Some("ping") => {
                                    let pong = json!({
                                        "type": "pong",
                                        "timestamp": json_msg.get("timestamp"),
                                        "server_timestamp": std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_millis()
                                    });
                                    if let Err(_) = control_tx_clone.send(pong.to_string()).await {
                                        break;
                                    }
                                }
                                Some("request_keyframe") => {
                                    let encoder = encoder_clone2.lock().await;
                                    encoder.force_keyframe();
                                    info!("🔑 Keyframe requested - forcing next frame");
                                }
                                Some("performance_mode") => {
                                    if let Some(mode_str) = json_msg.get("mode").and_then(|m| m.as_str()) {
                                        match mode_str {
                                            "gaming" => {
                                                let mut mode = performance_mode_clone3.write();
                                                *mode = PerformanceMode::Gaming;
                                                info!("🎮 Switched to GAMING performance mode");
                                            }
                                            "ultra" => {
                                                let mut mode = performance_mode_clone3.write();
                                                *mode = PerformanceMode::UltraLowLatency;
                                                info!("⚡ Switched to ULTRA-LOW LATENCY mode");
                                            }
                                            "balanced" => {
                                                let mut mode = performance_mode_clone3.write();
                                                *mode = PerformanceMode::Balanced;
                                                info!("⚖️  Switched to BALANCED mode");
                                            }
                                            _ => warn!("Unknown performance mode: {}", mode_str)
                                        }
                                    }
                                }
                                Some("emergency_reset") => {
                                    emergency_mode_flag.store(false, Ordering::Relaxed);
                                    let mut mode = performance_mode_clone3.write();
                                    *mode = PerformanceMode::UltraLowLatency;
                                    info!("🔄 Emergency mode reset - returning to ultra-low latency");
                                }
                                Some("network_stats") => {
                                    if let Some(stats) = json_msg.get("stats") {
                                        if let Ok(net_stats) = serde_json::from_value::<NetworkStats>(stats.clone()) {
                                            let mut network_stats = network_stats_clone.write();
                                            *network_stats = net_stats;
                                            
                                            // Adaptive performance based on network conditions
                                            if network_stats.latency > 100 || network_stats.packet_loss > 2.0 {
                                                let mut mode = performance_mode_clone.write();
                                                if !matches!(*mode, PerformanceMode::Balanced | PerformanceMode::Emergency) {
                                                    *mode = PerformanceMode::Balanced;
                                                    info!("🌐 Network issues detected - switching to balanced mode");
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    debug!("Unknown ultra message type: {}", text);
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("Ultra WebSocket connection closed by client");
                        break;
                    }
                    Err(e) => {
                        error!("Ultra WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });
        
        // Wait for completion or stop signal with ultra-fast response
        tokio::select! {
            _ = streaming_task => info!("Ultra streaming task completed"),
            _ = send_task => info!("Ultra send task completed"),  
            _ = receive_task => info!("Ultra receive task completed"),
            _ = async {
                if let Some(mut stop_rx) = stop_rx {
                    stop_rx.recv().await.ok();
                }
            } => info!("Ultra stop signal received"),
        }
        
        info!("🏁 ULTRA-LOW LATENCY streaming session ended");
    }
}

/// Simple fallback capture function that doesn't require self - with debugging and CORRECT FORMAT
async fn fallback_simple_capture() -> Result<Option<Vec<u8>>, String> {
    use crate::core::capture::ScreenCapture;
    
    debug!("🔍 [FALLBACK] Starting simple capture...");
    
    // Simple screen capture
    let monitors = ScreenCapture::get_all_monitors().map_err(|e| format!("Monitor error: {}", e))?;
    if monitors.is_empty() {
        return Err("No monitors available".to_string());
    }
    
    let mut capture = ScreenCapture::new(Some(0)).map_err(|e| format!("Capture init error: {}", e))?;
    
    match capture.capture_raw() {
        Ok(image_data) => {
            debug!("🔍 [FALLBACK] Raw capture successful: {} bytes", image_data.len());
            
            // Create frame data in the format expected by kvm-client.js
            let mut frame_data = Vec::new();
            
            // Add header expected by client: 0xAABB01 for normal frame (KEYFRAME)
            frame_data.extend_from_slice(&[0xAA, 0xBB, 0x01]); // Header - ALWAYS use keyframe for simplicity
            debug!("🔍 [FALLBACK] Added frame header: [0xAA, 0xBB, 0x01]");
            
            // Static frame number for debugging consistency
            let frame_number: u64 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            
            // HIGHER QUALITY: Use 2x2 blocks instead of 4x4 for better resolution
            let original_width = 1920;
            let original_height = 1080;
            let bytes_per_pixel = 4; // RGBA
            let block_size = 2; // 2x2 blocks for higher quality (4:1 compression)
            let new_width = original_width / block_size; // 960 pixels wide
            let new_height = original_height / block_size; // 540 pixels tall
            
            debug!("🔍 [FALLBACK] Processing {}x{} -> {}x{} with {}x{} blocks", 
                   original_width, original_height, new_width, new_height, block_size, block_size);
            
            // Step 1: Create downsampled RGB data
            let mut rgb_data = Vec::new();
            let mut processed_blocks = 0;
            
            for y in (0..original_height).step_by(block_size) {
                for x in (0..original_width).step_by(block_size) {
                    let mut r_sum = 0u32;
                    let mut g_sum = 0u32;
                    let mut b_sum = 0u32;
                    let mut pixel_count = 0u32;
                    
                    // Average the block
                    for dy in 0..block_size {
                        for dx in 0..block_size {
                            let px = x + dx;
                            let py = y + dy;
                            
                            if px < original_width && py < original_height {
                                let offset = (py * original_width + px) * bytes_per_pixel;
                                if offset + 3 < image_data.len() {
                                    r_sum += image_data[offset] as u32;     // R
                                    g_sum += image_data[offset + 1] as u32; // G
                                    b_sum += image_data[offset + 2] as u32; // B
                                    pixel_count += 1;
                                }
                            }
                        }
                    }
                    
                    // Calculate average and add to RGB data
                    if pixel_count > 0 {
                        rgb_data.push((r_sum / pixel_count) as u8);
                        rgb_data.push((g_sum / pixel_count) as u8);
                        rgb_data.push((b_sum / pixel_count) as u8);
                        processed_blocks += 1;
                    } else {
                        // Fallback for edge cases
                        rgb_data.extend_from_slice(&[0, 0, 0]);
                    }
                }
            }
            
            debug!("🔍 [FALLBACK] Processed {} blocks, RGB data: {} bytes", 
                   processed_blocks, rgb_data.len());
            
            // Step 2: Create proper RLE delta format as expected by KVM client fastApplyDelta()
            // Client expects: [change_count (4 bytes)] + [block_index (4 bytes) + R + G + B] per change
            let mut rle_changes = Vec::new();
            let total_pixels = (new_width * new_height) as usize;
            
            debug!("🔍 [FALLBACK] Creating RLE delta format for {} total pixels ({}x{})", 
                   total_pixels, new_width, new_height);
            
            // For simplicity, mark all blocks as changed (full frame update)
            let pixels_per_change = rgb_data.len() / 3; // Each pixel is 3 bytes (RGB)
            
            for pixel_idx in 0..pixels_per_change {
                let rgb_offset = pixel_idx * 3;
                if rgb_offset + 2 < rgb_data.len() {
                    // Add change entry: [pixel_index (4 bytes LE)] + [R, G, B]
                    rle_changes.extend_from_slice(&(pixel_idx as u32).to_le_bytes());
                    rle_changes.push(rgb_data[rgb_offset]);     // R
                    rle_changes.push(rgb_data[rgb_offset + 1]); // G
                    rle_changes.push(rgb_data[rgb_offset + 2]); // B
                }
            }
            
            let change_count = pixels_per_change as u32;
            
            debug!("🔍 [FALLBACK] RLE delta: {} changes, {} bytes per change, {} total RLE bytes", 
                   change_count, 7, rle_changes.len());
            
            // Build final RLE frame: [change_count (4 bytes)] + [changes...]
            let mut compressed_data = Vec::new();
            compressed_data.extend_from_slice(&change_count.to_le_bytes());
            compressed_data.extend_from_slice(&rle_changes);
            
            let compressed_length = compressed_data.len() as u32;
            
            debug!("🔍 [FALLBACK] RLE compression: {} RGB bytes -> {} RLE bytes", 
                   rgb_data.len(), compressed_data.len());
            
            debug!("🔍 [FALLBACK] Frame metadata - Width: {}, Height: {}, Frame: {}, Data: {} bytes", 
                   new_width, new_height, frame_number, compressed_length);
            
            // Write metadata in little-endian format as expected by client
            frame_data.extend_from_slice(&(new_width as u32).to_le_bytes());     // width (4 bytes)
            frame_data.extend_from_slice(&(new_height as u32).to_le_bytes());    // height (4 bytes) 
            frame_data.extend_from_slice(&frame_number.to_le_bytes());           // frame_number (8 bytes)
            frame_data.extend_from_slice(&compressed_length.to_le_bytes());      // compressed_length (4 bytes)
            
            // Add the RLE compressed frame data
            frame_data.extend_from_slice(&compressed_data);
            
            debug!("🔍 [FALLBACK] Final frame structure:");
            debug!("  - Header: 3 bytes [AA BB 01]");
            debug!("  - Width: 4 bytes [{}]", new_width);
            debug!("  - Height: 4 bytes [{}]", new_height);
            debug!("  - Frame number: 8 bytes [{}]", frame_number);
            debug!("  - Data length: 4 bytes [{}]", compressed_length);
            debug!("  - RLE data: {} bytes", compressed_data.len());
            debug!("  - Total frame: {} bytes", frame_data.len());
            
            info!("📸 [FALLBACK] RLE compressed capture: {}x{} -> {}x{} ({}KB RLE data, {}KB total frame)", 
                  original_width, original_height, new_width, new_height, 
                  compressed_data.len() / 1024, frame_data.len() / 1024);
            
            Ok(Some(frame_data))
        },
        Err(e) => {
            error!("🔴 [FALLBACK] Capture error: {}", e);
            Err(format!("Capture failed: {}", e))
        }
    }
}
