// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod input;
mod server;
mod utils;

use crate::server::WebSocketServer;
use local_ip_address::local_ip;
use std::sync::{Arc, Mutex};
use tauri::{Manager};
use tokio::runtime::Runtime;
use serde::Deserialize;

// Server options
#[derive(Debug, Deserialize, Default)]
struct ServerOptions {
    delta_encoding: Option<bool>,
    adaptive_quality: Option<bool>,
    encryption: Option<bool>,
    webrtc: Option<bool>,
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
fn start_server(app_handle: tauri::AppHandle, port: Option<u16>, options: Option<ServerOptions>) -> Result<String, String> {
    let port = port.unwrap_or(9921);
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let mut state = state.lock().unwrap();

    if state.running {
        return Err("Server is already running".to_string());
    }
    
    // Store options
    if let Some(opts) = options {
        state.options = opts;
    }

    let app_handle_clone = app_handle.clone();
    let server = state.runtime.block_on(async move {
        match WebSocketServer::new(port, app_handle_clone).await {
            Ok(server) => Ok(server),
            Err(e) => Err(format!("Failed to start server: {}", e)),
        }
    })?;

    state.server_handle = Some(server);
    state.port = port;
    state.running = true;

    // Get local IP address
    let ip = match local_ip() {
        Ok(ip) => ip.to_string(),
        Err(_) => "127.0.0.1".to_string(),
    };

    let url = format!("http://{}:{}/kvm", ip, port);
    Ok(url)
}

#[tauri::command]
fn stop_server(app_handle: tauri::AppHandle) -> Result<(), String> {
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let mut state = state.lock().unwrap();

    if !state.running {
        return Err("Server is not running".to_string());
    }

    if let Some(server) = state.server_handle.take() {
        state.runtime.block_on(async {
            server.shutdown().await;
        });
    }

    state.running = false;
    Ok(())
}

#[tauri::command]
fn get_server_status(app_handle: tauri::AppHandle) -> bool {
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let state = state.lock().unwrap();
    state.running
}

#[tauri::command]
fn get_server_url(app_handle: tauri::AppHandle) -> Result<String, String> {
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let state = state.lock().unwrap();

    if !state.running {
        return Err("Server is not running".to_string());
    }

    // Get local IP address
    let ip = match local_ip() {
        Ok(ip) => ip.to_string(),
        Err(_) => "127.0.0.1".to_string(),
    };

    let url = format!("http://{}:{}/kvm", ip, state.port);
    Ok(url)
}

fn main() {
    env_logger::init();
    
    tauri::Builder::default()
        .setup(|app| {
            // Initialize server state
            app.manage(Arc::new(Mutex::new(ServerState::new())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            get_server_status,
            get_server_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
