use axum::{
    extract::{ws::WebSocketUpgrade, Query},
    response::{Html, IntoResponse},
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
        ("{{fit_mode}}", if stretch { "contain".to_string() } else { "scale-down".to_string() }),
        ("{{display_mode}}", if remote_only { "position: absolute;".to_string() } else { "".to_string() }),
        ("{{mute_attr}}", if mute { "muted".to_string() } else { "".to_string() }),
        ("{{toolbar_class}}", if remote_only { "hidden".to_string() } else { "".to_string() }),
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
                "<html><body><h1>Error</h1><p>Failed to load KVM client template: {}</p></body></html>",
                err
            )
        }
    };

    Html(html)
}

pub async fn ws_handler(ws: WebSocketUpgrade, Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    // Extract monitor parameter
    let monitor = params.get("monitor").map(|v| v.parse::<usize>().unwrap_or(0)).unwrap_or(0);
    let codec = params.get("codec").map(|v| v.to_string()).unwrap_or_else(|| "h264".to_string());
    let audio = params.get("audio").map(|v| v == "true").unwrap_or(false);
    
    // Pass connection parameters to the WebSocket handler - use 'move' to take ownership
    ws.on_upgrade(move |socket| handle_socket_wrapper(socket, monitor, codec, audio))
}