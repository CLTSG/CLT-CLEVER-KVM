use axum::{
    extract::{ws::{WebSocketUpgrade, WebSocket}, Query},
    response::{Html, IntoResponse},
    http::{StatusCode, header},
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tokio::sync::broadcast;
use log::{info, debug, error};

use super::websocket::{handle_socket_wrapper, handle_socket_wrapper_with_stop};

fn get_web_client_path() -> PathBuf {
    // Try multiple possible locations for the web-client directory
    let possible_paths = vec![
        "web-client",                           // Current working directory
        "src-tauri/web-client",                 // From project root
        "../src-tauri/web-client",              // From dist directory  
        "./src-tauri/web-client",               // Alternative from root
    ];
    
    for path in possible_paths {
        let full_path = PathBuf::from(path);
        if full_path.exists() && full_path.is_dir() {
            return full_path;
        }
    }
    
    // Fallback to the default path
    PathBuf::from("web-client")
}

pub async fn kvm_client_handler(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    log::info!("KVM client page requested with parameters: {:?}", params);
    
    // Parse the query parameters
    let stretch = params.get("stretch").map(|v| v == "true").unwrap_or(false);
    let mute = params.get("mute").map(|v| v == "true").unwrap_or(false);
    let audio = params.get("audio").map(|v| v == "true").unwrap_or(false);
    let remote_only = params.get("remoteOnly").map(|v| v == "true").unwrap_or(false);
    let encryption = params.get("encryption").map(|v| v == "true").unwrap_or(false);
    let monitor = params.get("monitor").map(|v| v.parse::<usize>().unwrap_or(0)).unwrap_or(0);
    let codec = params.get("codec").unwrap_or(&"vp8".to_string()).clone();
    
    log::debug!("KVM client configuration - stretch: {}, mute: {}, audio: {}, monitor: {}, codec: {}", 
               stretch, mute, audio, monitor, codec);    // Prepare template replacements
    let replacements = [
        ("{{stretch}}", stretch.to_string()),
        ("{{mute}}", mute.to_string()),
        ("{{audio}}", audio.to_string()),
        ("{{remote_only}}", remote_only.to_string()),
        ("{{encryption}}", encryption.to_string()),
        ("{{monitor}}", monitor.to_string()),
        ("{{codec}}", codec.to_string()),
        ("{{mute_attr}}", if mute { "muted".to_string() } else { "".to_string() }),
        ("{{stretch_checked}}", if stretch { "checked".to_string() } else { "".to_string() }),
        ("{{audio_checked}}", if audio { "checked".to_string() } else { "".to_string() }),
        ("{{mute_checked}}", if mute { "checked".to_string() } else { "".to_string() }),
    ];

    // Load the HTML template
    let web_client_base = get_web_client_path();
    let template_path = web_client_base.join("kvm-template.html");
    
    log::info!("Looking for template at: {:?}", template_path);
    
    let html = match fs::read_to_string(&template_path) {
        Ok(template) => {
            // Apply all replacements
            let mut result = template;
            for (pattern, replacement) in replacements.iter() {
                result = result.replace(pattern, replacement);
            }
            result
        },
        Err(err) => {
            // If template loading fails, return a simple error page
            log::error!("Failed to load KVM template: {}", err);
            format!(
                r#"<!DOCTYPE html>
<html><body>
<h1>Error</h1>
<p>Failed to load KVM client template: {}</p>
<p>Please ensure the web-client directory is properly configured.</p>
</body></html>"#,
                err
            )
        }
    };

    Html(html)
}

// New handler for serving static files
pub async fn static_file_handler(
    axum::extract::Path(path): axum::extract::Path<String>
) -> impl IntoResponse {
    let web_client_base = get_web_client_path();
    let file_path = web_client_base.join(&path);
    
    log::info!("Looking for static file at: {:?}", file_path);
    
    match fs::read(&file_path) {
        Ok(contents) => {
            // Determine content type based on file extension
            let content_type = match path.split('.').last() {
                Some("css") => "text/css",
                Some("js") => "application/javascript",
                Some("html") => "text/html",
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("svg") => "image/svg+xml",
                Some("ico") => "image/x-icon",
                _ => "application/octet-stream",
            };
            
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type)],
                contents
            )
        },
        Err(_) => {
            (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/plain")],
                b"File not found".to_vec()
            )
        }
    }
}

pub async fn ws_handler(ws: WebSocketUpgrade, Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    // Extract monitor parameter
    let monitor = params.get("monitor").map(|v| v.parse::<usize>().unwrap_or(0)).unwrap_or(0);
    // Parse codec parameter and default to vp8 if not specified
    let codec = params.get("codec").unwrap_or(&"vp8".to_string()).clone();
    let audio = params.get("audio").map(|v| v == "true").unwrap_or(false);
    
    log::debug!("WebSocket connection - monitor: {}, codec: {}, audio: {}", monitor, codec, audio);
    
    // Use platform-specific handlers to avoid Send trait issues
    #[cfg(target_os = "macos")]
    {
        ws.on_upgrade(move |socket| async move {
            let stop_rx = tokio::sync::broadcast::channel(1).1;
            if let Err(e) = handle_macos_socket_fallback(socket, monitor, codec, audio, stop_rx).await {
                log::error!("macOS WebSocket handler failed: {}", e);
            }
        })
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        ws.on_upgrade(move |socket| handle_socket_wrapper(socket, monitor, codec, audio))
    }
}

pub async fn ws_handler_with_stop(
    ws: WebSocketUpgrade, 
    Query(params): Query<HashMap<String, String>>,
    stop_rx: broadcast::Receiver<()>
) -> impl IntoResponse {
    // Extract monitor parameter
    let monitor = params.get("monitor").map(|v| v.parse::<usize>().unwrap_or(0)).unwrap_or(0);
    // Parse codec parameter and default to vp8 if not specified
    let codec = params.get("codec").unwrap_or(&"vp8".to_string()).clone();
    let audio = params.get("audio").map(|v| v == "true").unwrap_or(false);
    
    log::info!("WebSocket connection request - monitor: {}, codec: {}, audio: {}", monitor, codec, audio);
    log::debug!("WebSocket query parameters: {:?}", params);
    
    // Platform-specific handling to avoid Send trait issues on macOS
    #[cfg(target_os = "macos")]
    {
        // On macOS, create a simple handler without encoders
        ws.on_upgrade(move |socket| async move {
            log::info!("WebSocket connection established (macOS fallback)");
            if let Err(e) = handle_macos_socket_fallback(socket, monitor, codec, audio, stop_rx).await {
                error!("macOS WebSocket connection failed: {}", e);
            }
        })
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        // On other platforms, use the original handler
        ws.on_upgrade(move |socket| {
            log::info!("WebSocket connection established");
            handle_socket_wrapper_with_stop(socket, monitor, codec, audio, stop_rx)
        })
    }
}

#[cfg(target_os = "macos")]
async fn handle_macos_socket_fallback(
    socket: WebSocket, 
    monitor: usize, 
    codec: String, 
    audio: bool,
    _stop_rx: broadcast::Receiver<()>
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};
    use xcap::Monitor;
    
    info!("🍎 Using macOS fallback streaming for monitor {}", monitor);
    
    let (mut sender, mut receiver) = socket.split();
    
    // Send initial info
    let _ = sender.send(Message::Text(serde_json::json!({
        "type": "server_info",
        "version": "3.0.0-macos-fallback",
        "platform": "macOS",
        "streaming_mode": "fallback"
    }).to_string())).await;
    
    // Simple capture loop without complex encoders
    let capture_task = tokio::spawn(async move {
        let mut frame_count = 0u64;
        
        loop {
            // Get monitors fresh each time to avoid Send issues
            let monitors = match Monitor::all() {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to get monitors: {:?}", e);
                    break;
                }
            };
            
            let monitor = match monitors.get(monitor) {
                Some(m) => m,
                None => {
                    error!("Monitor {} not found", monitor);
                    break;
                }
            };
            
            match monitor.capture_image() {
                Ok(image) => {
                    // Create simple frame format
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
                    
                    if frame_count % 30 == 0 {
                        info!("🍎 macOS fallback: sent {} frames", frame_count);
                    }
                },
                Err(e) => {
                    error!("Capture failed: {:?}", e);
                }
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(33)).await; // ~30 FPS
        }
        
        info!("🍎 macOS fallback capture ended");
    });
    
    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                debug!("Received control message: {}", text);
                // Handle control messages if needed
            },
            Ok(Message::Binary(_)) => {
                // Handle binary messages if needed
            },
            Ok(Message::Close(_)) => {
                info!("WebSocket connection closed");
                break;
            },
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            },
            _ => {}
        }
    }
    
    capture_task.abort();
    Ok(())
}