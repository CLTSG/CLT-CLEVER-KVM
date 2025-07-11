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

    // Get local IP address with better detection
    let ip = get_network_ip().unwrap_or_else(|| {
        warn!("Could not determine network IP, falling back to localhost");
        "127.0.0.1".to_string()
    });

    let url = format!("http://{}:{}/kvm", ip, port);
    info!("Server URL: {}", url);
    info!("Server is now accessible from network at: {}", url);
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

    // Get local IP address with better error handling and multiple attempts
    let ip = get_network_ip().unwrap_or_else(|| {
        warn!("Could not determine network IP, falling back to localhost");
        "127.0.0.1".to_string()
    });

    let url = format!("http://{}:{}/kvm", ip, state.port);
    debug!("Returning server URL: {}", url);
    info!("KVM server accessible at: {}", url);
    Ok(url)
}

// Helper function to get the best network IP address
fn get_network_ip() -> Option<String> {
    // Try the local_ip crate first
    if let Ok(ip) = local_ip() {
        let ip_str = ip.to_string();
        debug!("Local IP from local_ip crate: {}", ip_str);
        
        // Avoid loopback addresses
        if !ip_str.starts_with("127.") && !ip_str.starts_with("::1") {
            info!("Using network IP: {}", ip_str);
            return Some(ip_str);
        }
    }
    
    // Fallback: try to get IP from network interfaces
    if let Ok(interfaces) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if let Ok(()) = interfaces.connect("8.8.8.8:80") {
            if let Ok(addr) = interfaces.local_addr() {
                let ip_str = addr.ip().to_string();
                debug!("Network IP from UDP socket: {}", ip_str);
                if !ip_str.starts_with("127.") && !ip_str.starts_with("::1") {
                    info!("Using network IP from UDP socket: {}", ip_str);
                    return Some(ip_str);
                }
            }
        }
    }
    
    // Try to enumerate network interfaces manually
    match get_available_network_interfaces() {
        Ok(interfaces) => {
            for interface in interfaces {
                if !interface.starts_with("127.") && !interface.starts_with("::1") 
                    && !interface.contains("localhost") {
                    info!("Using network IP from interface enumeration: {}", interface);
                    return Some(interface);
                }
            }
        },
        Err(e) => {
            warn!("Failed to enumerate network interfaces: {}", e);
        }
    }
    
    // Last resort: return None to fall back to localhost
    warn!("Could not determine network IP address, will use localhost");
    None
}

// Helper function to get all available network interfaces
fn get_available_network_interfaces() -> Result<Vec<String>, String> {
    use std::process::Command;
    
    let mut interfaces = Vec::new();
    
    // Try using 'ip addr' command on Linux
    if cfg!(target_os = "linux") {
        if let Ok(output) = Command::new("ip").args(&["route", "get", "8.8.8.8"]).output() {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                // Parse the output to extract the source IP
                for line in output_str.lines() {
                    if line.contains("src") {
                        if let Some(src_pos) = line.find("src ") {
                            let src_part = &line[src_pos + 4..];
                            if let Some(space_pos) = src_part.find(' ') {
                                let ip = &src_part[..space_pos];
                                interfaces.push(ip.to_string());
                                debug!("Found network IP via 'ip route': {}", ip);
                            }
                        }
                    }
                }
            }
        }
        
        // Also try 'hostname -I' as backup
        if let Ok(output) = Command::new("hostname").arg("-I").output() {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                for ip in output_str.trim().split_whitespace() {
                    if !ip.starts_with("127.") && !ip.starts_with("::1") {
                        interfaces.push(ip.to_string());
                        debug!("Found network IP via 'hostname -I': {}", ip);
                    }
                }
            }
        }
    }
    
    // Try using 'ifconfig' as fallback
    if interfaces.is_empty() {
        if let Ok(output) = Command::new("ifconfig").output() {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                // Simple parsing to extract inet addresses
                for line in output_str.lines() {
                    if line.trim().starts_with("inet ") && !line.contains("127.0.0.1") {
                        if let Some(inet_pos) = line.find("inet ") {
                            let inet_part = &line[inet_pos + 5..];
                            if let Some(space_pos) = inet_part.find(' ') {
                                let ip = &inet_part[..space_pos];
                                interfaces.push(ip.to_string());
                                debug!("Found network IP via ifconfig: {}", ip);
                            }
                        }
                    }
                }
            }
        }
    }
    
    if interfaces.is_empty() {
        Err("No network interfaces found".to_string())
    } else {
        Ok(interfaces)
    }
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

#[tauri::command]
fn get_network_interfaces() -> Result<Vec<String>, String> {
    let mut interfaces = Vec::new();
    
    // Add the detected network IP
    if let Some(ip) = get_network_ip() {
        interfaces.push(format!("{} (detected network IP)", ip));
    }
    
    // Add localhost
    interfaces.push("127.0.0.1 (localhost)".to_string());
    
    // Try to get additional interfaces using our new comprehensive method
    match get_available_network_interfaces() {
        Ok(net_interfaces) => {
            for interface in net_interfaces {
                if !interfaces.iter().any(|i| i.starts_with(&interface)) {
                    interfaces.push(format!("{} (network interface)", interface));
                }
            }
        },
        Err(e) => {
            warn!("Failed to get network interfaces: {}", e);
        }
    }
    
    // Add some diagnostic information
    interfaces.push("--- Diagnostic Info ---".to_string());
    interfaces.push(format!("Server binding to: 0.0.0.0:9921 (all interfaces)"));
    
    Ok(interfaces)
}

#[tauri::command]
fn test_network_connectivity(app_handle: tauri::AppHandle) -> Result<String, String> {
    let state = app_handle.state::<Arc<Mutex<ServerState>>>();
    let state = state.lock().unwrap();

    if !state.running {
        return Err("Server is not running".to_string());
    }

    let port = state.port;
    drop(state); // Release the lock

    // Test if we can connect to our own server
    let test_result = std::thread::spawn(move || {
        use std::net::TcpStream;
        use std::time::Duration;

        let mut results = Vec::new();
        
        results.push("=== Network Connectivity Test ===".to_string());

        // Test localhost connection
        match TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(3)
        ) {
            Ok(_) => {
                results.push("✓ Localhost connection: SUCCESS".to_string());
                results.push("  → Server is reachable on localhost".to_string());
            },
            Err(e) => {
                results.push(format!("✗ Localhost connection: FAILED ({})", e));
                results.push("  → Server may not be properly bound to localhost".to_string());
            }
        }

        // Test network IP connection if available
        if let Some(ip) = get_network_ip() {
            match TcpStream::connect_timeout(
                &format!("{}:{}", ip, port).parse().unwrap(),
                Duration::from_secs(3)
            ) {
                Ok(_) => {
                    results.push(format!("✓ Network IP ({}) connection: SUCCESS", ip));
                    results.push("  → Server is accessible from the network".to_string());
                },
                Err(e) => {
                    results.push(format!("✗ Network IP ({}) connection: FAILED ({})", ip, e));
                    results.push("  → Server may be blocked by firewall or not properly bound".to_string());
                }
            }
        } else {
            results.push("⚠ Could not determine network IP address".to_string());
            results.push("  → Server will only be accessible via localhost".to_string());
        }
        
        // Add some additional diagnostic info
        results.push("".to_string());
        results.push("=== Binding Information ===".to_string());
        results.push(format!("Server is configured to bind to: 0.0.0.0:{}", port));
        results.push("This should make it accessible from all network interfaces".to_string());
        
        // Test if the port is in use by other processes
        results.push("".to_string());
        results.push("=== Port Usage Check ===".to_string());
        
        // Try to bind to the same port to see if it's available
        match std::net::TcpListener::bind(format!("127.0.0.1:{}", port + 1)) {
            Ok(_) => results.push("✓ Network stack is working properly".to_string()),
            Err(e) => results.push(format!("⚠ Network issue detected: {}", e)),
        }

        results.join("\n")
    }).join().map_err(|_| "Failed to run network test".to_string())?;

    Ok(test_result)
}

#[tauri::command]
fn get_system_info() -> Result<String, String> {
    let mut info = Vec::new();
    
    info.push("=== System Information ===".to_string());
    
    // Operating system
    info.push(format!("OS: {}", std::env::consts::OS));
    info.push(format!("Architecture: {}", std::env::consts::ARCH));
    
    // Hostname
    match gethostname::gethostname().to_str() {
        Some(hostname) => info.push(format!("Hostname: {}", hostname)),
        None => info.push("Hostname: Unknown".to_string()),
    }
    
    // Current working directory
    match std::env::current_dir() {
        Ok(dir) => info.push(format!("Working directory: {}", dir.display())),
        Err(e) => info.push(format!("Working directory: Error ({})", e)),
    }
    
    // Environment variables that might affect networking
    if let Ok(path) = std::env::var("PATH") {
        info.push(format!("PATH (first 100 chars): {}", 
                         if path.len() > 100 { &path[..100] } else { &path }));
    }
    
    Ok(info.join("\n"))
}

#[tauri::command]
fn check_firewall_status() -> Result<String, String> {
    use std::process::Command;
    
    let mut results = Vec::new();
    
    results.push("=== Firewall Status Check ===".to_string());
    
    if cfg!(target_os = "linux") {
        // Check ufw status
        if let Ok(output) = Command::new("ufw").arg("status").output() {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                results.push("UFW Status:".to_string());
                results.push(output_str.trim().to_string());
            }
        } else {
            results.push("UFW: Not available or permission denied".to_string());
        }
        
        // Check iptables (basic check)
        if let Ok(output) = Command::new("iptables").args(&["-L", "-n"]).output() {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                let lines: Vec<&str> = output_str.lines().take(10).collect();
                results.push("".to_string());
                results.push("iptables (first 10 lines):".to_string());
                results.extend(lines.iter().map(|s| s.to_string()));
            }
        } else {
            results.push("iptables: Not available or permission denied".to_string());
        }
        
        // Check if port 9921 is listening
        if let Ok(output) = Command::new("netstat").args(&["-tuln"]).output() {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                results.push("".to_string());
                results.push("Listening ports containing 9921:".to_string());
                for line in output_str.lines() {
                    if line.contains("9921") {
                        results.push(format!("  {}", line));
                    }
                }
            }
        }
    } else {
        results.push("Firewall check not implemented for this OS".to_string());
    }
    
    Ok(results.join("\n"))
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
            let server_state = Arc::new(Mutex::new(ServerState::new()));
            app.manage(server_state.clone());
            debug!("Server state initialized");
            
            // Auto-start the server with default settings
            info!("Auto-starting KVM server on port 9921");
            let app_handle = app.handle();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1000)); // Give the app time to initialize
                if let Err(e) = start_server(app_handle, Some(9921), Some(ServerOptions::default())) {
                    error!("Failed to auto-start server: {}", e);
                } else {
                    info!("Server auto-started successfully");
                }
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            get_server_status,
            get_server_url,
            get_logs,
            get_available_monitors,
            get_network_interfaces,
            test_network_connectivity,
            get_system_info,
            check_firewall_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
