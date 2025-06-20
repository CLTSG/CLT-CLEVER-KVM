use axum::{
    extract::{ws::WebSocketUpgrade, Query},
    response::{Html, IntoResponse},
    http::{StatusCode, header},
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::websocket::handle_socket_wrapper;

pub async fn kvm_client_handler(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    // Parse the query parameters
    let stretch = params.get("stretch").map(|v| v == "true").unwrap_or(false);
    let mute = params.get("mute").map(|v| v == "true").unwrap_or(false);
    let audio = params.get("audio").map(|v| v == "true").unwrap_or(false);
    let remote_only = params.get("remoteOnly").map(|v| v == "true").unwrap_or(false);
    let encryption = params.get("encryption").map(|v| v == "true").unwrap_or(false);
    let monitor = params.get("monitor").map(|v| v.parse::<usize>().unwrap_or(0)).unwrap_or(0);
    let codec = params.get("codec").map(|v| v.as_str()).unwrap_or("h264");

    // Prepare template replacements
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
    let template_path = Path::new("web-client/kvm-template.html");
    
    let html = match fs::read_to_string(template_path) {
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
    let file_path = format!("web-client/{}", path);
    
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
    let codec = params.get("codec").map(|v| v.to_string()).unwrap_or_else(|| "h264".to_string());
    let audio = params.get("audio").map(|v| v == "true").unwrap_or(false);
    
    // Pass connection parameters to the WebSocket handler - use 'move' to take ownership
    ws.on_upgrade(move |socket| handle_socket_wrapper(socket, monitor, codec, audio))
}