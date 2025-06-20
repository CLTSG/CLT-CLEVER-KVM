// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod input;
mod server;
mod utils;
mod logging;
mod codec;
mod audio;
mod system_check; // Add system check module

use crate::server::WebSocketServer;
use crate::capture::ScreenCapture;
use local_ip_address::local_ip;
use std::sync::{Arc, Mutex};
use tauri::{Manager};
use tokio::runtime::Runtime;
use serde::{Deserialize, Serialize};
use log::{info, warn, error, debug};

const APP_NAME: &str = "clever-kvm";
const LOG_ROTATE_SIZE_MB: u64 = 10;

// Server options
#[derive(Debug, Deserialize, Default)]
struct ServerOptions {
    delta_encoding: Option<bool>,
    adaptive_quality: Option<bool>,
    encryption: Option<bool>,
    webrtc: Option<bool>,
    h264: Option<bool>,
    monitor: Option<usize>,
}

// Monitor info for frontend
#[derive(Debug, Serialize)]
struct MonitorInfo {
    id: String,
    name: String,
    is_primary: bool,
    width: usize,
    height: usize,
    position_x: i32,
    position_y: i32,
}

// Shared state between Tauri and WebSocket server
pub struct ServerState {
    runtime: Runtime,
    server_handle: Option<WebSocketServer>,
    port: u16,
    running: bool,
    options: ServerOptions,
}

impl ServerState {
    fn new() -> Self {
        Self {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime"),
            server_handle: None,
            port: 9921, // Default port
            running: false,
            options: ServerOptions::default(),
        }
    }
}

#[tauri::command]
fn get_available_monitors() -> Result<Vec<MonitorInfo>, String> {
    match ScreenCapture::get_all_monitors() {
        Ok(monitors) => {
            // Convert to frontend-friendly structure
            let frontend_monitors = monitors.into_iter().map(|m| MonitorInfo {
                id: m.id,
                name: m.name,
                is_primary: m.is_primary,
                width: m.width,
                height: m.height,
                position_x: m.position_x,
                position_y: m.position_y,
            }).collect();
            
            Ok(frontend_monitors)
        },
        Err(e) => Err(format!("Failed to get monitors: {}", e)),
    }
}

#[tauri::command]
fn start_server(app_handle: tauri::AppHandle, port: Option<u16>, options: Option<ServerOptions>) -> Result<String, String> {
    let port = port.unwrap_or(9921);
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let mut state = state.lock().unwrap();

    if state.running {
        warn!("Attempted to start server when already running");
        return Err("Server is already running".to_string());
    }
    
    // Store options
    if let Some(opts) = options {
        debug!("Server options: delta_encoding={:?}, adaptive_quality={:?}, encryption={:?}, webrtc={:?}, h264={:?}, monitor={:?}",
               opts.delta_encoding, opts.adaptive_quality, opts.encryption, opts.webrtc, opts.h264, opts.monitor);
        state.options = opts;
    }

    info!("Starting KVM server on port {}", port);
    let app_handle_clone = app_handle.clone();
    let server = state.runtime.block_on(async move {
        match WebSocketServer::new(port, app_handle_clone).await {
            Ok(server) => {
                info!("Server started successfully");
                Ok(server)
            },
            Err(e) => {
                error!("Failed to start server: {}", e);
                Err(format!("Failed to start server: {}", e))
            },
        }
    })?;

    state.server_handle = Some(server);
    state.port = port;
    state.running = true;

    // Get local IP address
    let ip = match local_ip() {
        Ok(ip) => {
            debug!("Local IP address: {}", ip);
            ip.to_string()
        },
        Err(e) => {
            warn!("Failed to determine local IP address: {}", e);
            "127.0.0.1".to_string()
        },
    };

    let url = format!("http://{}:{}/kvm", ip, port);
    info!("Server URL: {}", url);
    Ok(url)
}

#[tauri::command]
fn stop_server(app_handle: tauri::AppHandle) -> Result<(), String> {
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let mut state = state.lock().unwrap();

    if !state.running {
        warn!("Attempted to stop server when not running");
        return Err("Server is not running".to_string());
    }

    info!("Stopping KVM server");
    if let Some(server) = state.server_handle.take() {
        state.runtime.block_on(async {
            server.shutdown().await;
        });
        info!("Server stopped successfully");
    }

    state.running = false;
    Ok(())
}

#[tauri::command]
fn get_server_status(app_handle: tauri::AppHandle) -> bool {
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let state = state.lock().unwrap();
    debug!("Server status requested: {}", state.running);
    state.running
}

#[tauri::command]
fn get_server_url(app_handle: tauri::AppHandle) -> Result<String, String> {
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let state = state.lock().unwrap();

    if !state.running {
        warn!("URL requested but server is not running");
        return Err("Server is not running".to_string());
    }

    // Get local IP address
    let ip = match local_ip() {
        Ok(ip) => ip.to_string(),
        Err(e) => {
            warn!("Failed to determine local IP address: {}", e);
            "127.0.0.1".to_string()
        },
    };

    let url = format!("http://{}:{}/kvm", ip, state.port);
    debug!("Returning server URL: {}", url);
    Ok(url)
}

#[tauri::command]
fn get_logs() -> Result<(String, String), String> {
    if let Some((debug_path, error_path)) = logging::get_log_paths(APP_NAME) {
        let debug_content = std::fs::read_to_string(debug_path)
            .map_err(|e| format!("Failed to read debug log: {}", e))?;
            
        let error_content = std::fs::read_to_string(error_path)
            .map_err(|e| format!("Failed to read error log: {}", e))?;
            
        Ok((debug_content, error_content))
    } else {
        Err("Could not determine log file paths".to_string())
    }
}

fn main() {
    // Initialize logging
    if let Err(e) = logging::init(APP_NAME) {
        eprintln!("Failed to initialize logging: {}", e);
    }
    
    // Rotate logs if they're too big
    if let Err(e) = logging::rotate_logs(APP_NAME, LOG_ROTATE_SIZE_MB) {
        eprintln!("Failed to rotate logs: {}", e);
    }
    
    info!("Clever KVM starting up");
    debug!("Running in {} mode", if cfg!(debug_assertions) { "DEBUG" } else { "RELEASE" });
    
    tauri::Builder::default()
        .setup(|app| {
            info!("Setting up Tauri application");
            // Initialize server state
            app.manage(Arc::new(Mutex::new(ServerState::new())));
            debug!("Server state initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            get_server_status,
            get_server_url,
            get_logs,
            get_available_monitors
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
