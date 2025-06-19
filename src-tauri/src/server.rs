use crate::capture::ScreenCapture;
use crate::input::{InputEvent, InputHandler};
use crate::utils::{EncryptionManager, compress_data};
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Query},
    response::{Html, IntoResponse},
    routing::get,
    Router,
    handler::HandlerWithoutStateExt,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time,
    net::TcpListener
};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use tower_http::trace::TraceLayer;
use base64::{Engine as _, engine::general_purpose};
use tower_http::services::ServeDir;
use std::convert::Infallible;
use axum::http::{StatusCode, Response};
use axum::body::Body;

// Constants
const NETWORK_PERFORMANCE_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_QUALITY: u8 = 85;
const MIN_QUALITY: u8 = 25;
const MAX_QUALITY: u8 = 95;

#[derive(Debug, Deserialize)]
pub struct KvmParams {
    stretch: Option<String>,
    mute: Option<String>,
    audio: Option<String>,
    remote_only: Option<String>,
    encryption: Option<String>,
}

#[derive(Debug, Serialize)]
struct FrameData {
    width: usize,
    height: usize,
    image: String,
    timestamp: u128,
}

#[derive(Debug, Serialize)]
struct DeltaFrameData {
    tiles: HashMap<usize, String>,
    timestamp: u128,
}

#[derive(Debug, Serialize)]
struct ServerInfo {
    width: usize,
    height: usize,
    hostname: String,
    tile_width: usize,
    tile_height: usize,
    tile_size: usize,
}

#[derive(Debug, Deserialize, Clone)]
struct NetworkStats {
    latency: u32,  // in milliseconds
    bandwidth: f32, // in Mbps
    packet_loss: f32, // percentage
}

pub struct WebSocketServer {
    shutdown_tx: mpsc::Sender<()>,
    server_handle: JoinHandle<()>,
}

// Define a simple function that returns a 404 response
async fn handle_404() -> Result<Response<Body>, Infallible> {
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap())
}

impl WebSocketServer {
    pub async fn new(port: u16, _app_handle: AppHandle) -> Result<Self, String> {
        // Channel for shutdown signal
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        
        // Set up the router
        let app = Router::new()
            .route("/ws", get(ws_handler))
            .route("/kvm", get(kvm_client_handler))
            .fallback_service(
                ServeDir::new("web-client")
                    .append_index_html_on_directories(true)
                    .not_found_service(handle_404.into_service())
            )
            .layer(TraceLayer::new_for_http());

        // Create TCP listener
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let listener = match TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(e) => return Err(format!("Failed to bind to address: {}", e)),
        };
        
        log::info!("WebSocket server listening on {}", addr);

        // Create server with axum
        let server = axum::serve(
            listener,
            app.into_make_service()
        ).with_graceful_shutdown(async move {
            shutdown_rx.recv().await;
        });

        // Spawn the server task
        let server_handle = tokio::spawn(async move {
            if let Err(e) = server.await {
                log::error!("Server error: {}", e);
            }
        });

        Ok(WebSocketServer {
            shutdown_tx,
            server_handle,
        })
    }

    pub async fn shutdown(self) {
        // Send shutdown signal
        if let Err(e) = self.shutdown_tx.send(()).await {
            log::error!("Failed to send shutdown signal: {}", e);
        }

        // Wait for server to shutdown
        if let Err(e) = self.server_handle.await {
            log::error!("Failed to join server task: {}", e);
        }

        log::info!("WebSocket server shut down");
    }
}

async fn kvm_client_handler(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    // Parse the query parameters
    let stretch = params.get("stretch").map(|v| v == "true").unwrap_or(false);
    let mute = params.get("mute").map(|v| v == "true").unwrap_or(false);
    let audio = params.get("audio").map(|v| v == "true").unwrap_or(false);
    let remote_only = params.get("remoteOnly").map(|v| v == "true").unwrap_or(false);
    let encryption = params.get("encryption").map(|v| v == "true").unwrap_or(false);

    // Generate HTML for the KVM client
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Clever KVM</title>
    <style>
        body, html {{ 
            margin: 0; 
            padding: 0; 
            height: 100%; 
            overflow: hidden; 
            background-color: #000;
            display: flex;
            flex-direction: column;
        }}
        #screen {{
            flex: 1;
            display: flex;
            justify-content: center;
            align-items: center;
            overflow: hidden;
            position: relative;
        }}
        #remote-screen {{
            {display_mode}
            max-width: 100%;
            max-height: 100%;
            object-fit: {fit_mode};
        }}
        #canvas-layer {{
            position: absolute;
            top: 0;
            left: 0;
            pointer-events: none;
        }}
        #toolbar {{
            background-color: #333;
            color: white;
            padding: 5px;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }}
        #status {{
            padding: 0 10px;
        }}
        #toolbar button {{
            background-color: #555;
            color: white;
            border: none;
            padding: 5px 10px;
            margin: 0 5px;
            cursor: pointer;
            border-radius: 3px;
        }}
        #toolbar button:hover {{
            background-color: #777;
        }}
        .hidden {{
            display: none !important;
        }}
        #network-stats {{
            position: absolute;
            bottom: 10px;
            right: 10px;
            background-color: rgba(0,0,0,0.7);
            color: white;
            padding: 5px;
            font-size: 12px;
            border-radius: 3px;
            font-family: monospace;
            z-index: 100;
        }}
    </style>
</head>
<body>
    <div id="toolbar" class="{toolbar_class}">
        <div id="status">Connecting...</div>
        <div>
            <button id="fullscreen-btn">Fullscreen</button>
            <button id="settings-btn">Settings</button>
            <button id="disconnect-btn">Disconnect</button>
        </div>
    </div>
    <div id="screen">
        <img id="remote-screen" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=">
        <canvas id="canvas-layer"></canvas>
        <div id="network-stats" class="hidden">
            FPS: <span id="fps">0</span> | 
            Latency: <span id="latency">0</span>ms | 
            Quality: <span id="quality">0</span>%
        </div>
    </div>
    <audio id="remote-audio" autoplay {mute_attr}></audio>

    <script>
        // KVM Client Configuration
        const config = {{
            stretch: {stretch},
            mute: {mute},
            audio: {audio},
            remoteOnly: {remote_only},
            encryption: {encryption}
        }};

        // Elements
        const screen = document.getElementById('remote-screen');
        const canvasLayer = document.getElementById('canvas-layer');
        const ctx = canvasLayer.getContext('2d');
        const status = document.getElementById('status');
        const audioElement = document.getElementById('remote-audio');
        const fullscreenBtn = document.getElementById('fullscreen-btn');
        const disconnectBtn = document.getElementById('disconnect-btn');
        const networkStats = document.getElementById('network-stats');
        const fpsElement = document.getElementById('fps');
        const latencyElement = document.getElementById('latency');
        const qualityElement = document.getElementById('quality');

        // Connection
        let ws;
        let connected = false;
        let lastFrame = 0;
        let frameCount = 0;
        let lastFpsUpdate = Date.now();
        let screenWidth = 0;
        let screenHeight = 0;
        let tileWidth = 0;
        let tileHeight = 0;
        let tileSize = 0;
        let latency = 0;
        let lastPingTime = 0;
        let pingInterval;
        let qualityLevel = 85;
        
        // Tile cache
        const tileImages = new Map();
        let totalTiles = 0;

        // Connect to WebSocket server
        function connect() {{
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${{protocol}}//${{window.location.host}}/ws`;
            
            ws = new WebSocket(wsUrl);
            
            ws.onopen = () => {{
                connected = true;
                status.textContent = 'Connected';
                console.log('WebSocket connection established');
                
                // Start sending ping messages to measure latency
                pingInterval = setInterval(sendPing, 2000);
                
                // Show network stats
                networkStats.classList.remove('hidden');
            }};
            
            ws.onclose = () => {{
                connected = false;
                status.textContent = 'Disconnected';
                console.log('WebSocket connection closed');
                
                // Clear ping interval
                clearInterval(pingInterval);
                
                // Hide network stats
                networkStats.classList.add('hidden');
                
                // Try to reconnect after a delay
                setTimeout(connect, 3000);
            }};
            
            ws.onerror = (error) => {{
                console.error('WebSocket error:', error);
                status.textContent = 'Connection error';
            }};
            
            ws.onmessage = (event) => {{
                try {{
                    const data = JSON.parse(event.data);
                    
                    if (data.type === 'frame') {{
                        // Update screen dimensions if needed
                        if (screenWidth !== data.width || screenHeight !== data.height) {{
                            screenWidth = data.width;
                            screenHeight = data.height;
                        }}
                        
                        // Update the screen image
                        screen.src = 'data:image/jpeg;base64,' + data.image;
                        
                        // Update FPS counter
                        frameCount++;
                        const now = Date.now();
                        if (now - lastFpsUpdate > 1000) {{
                            const fps = frameCount / ((now - lastFpsUpdate) / 1000);
                            fpsElement.textContent = Math.round(fps);
                            frameCount = 0;
                            lastFpsUpdate = now;
                        }}
                        
                        lastFrame = Date.now();
                    }} else if (data.type === 'delta') {{
                        // Delta frame with tiles
                        if (data.tiles) {{
                            // Handle the special case of a full frame
                            if (data.tiles["4294967295"]) {{ // 0xFFFFFFFF
                                screen.src = 'data:image/jpeg;base64,' + data.tiles["4294967295"];
                            }} else {{
                                // We got partial updates, apply them to the canvas
                                for (const [tileIndex, tileData] of Object.entries(data.tiles)) {{
                                    // Cache the tile image
                                    const tileImg = new Image();
                                    tileImg.onload = () => {{
                                        // Calculate tile position
                                        const tx = tileIndex % tileWidth;
                                        const ty = Math.floor(tileIndex / tileWidth);
                                        const x = tx * tileSize;
                                        const y = ty * tileSize;
                                        
                                        // Draw on canvas
                                        ctx.drawImage(tileImg, x, y);
                                        
                                        // If this is the first frame, copy the canvas to the image
                                        if (!screen.complete || screen.naturalWidth === 0) {{
                                            screen.src = canvasLayer.toDataURL('image/jpeg');
                                        }}
                                    }};
                                    tileImg.src = 'data:image/jpeg;base64,' + tileData;
                                    
                                    // Store in cache
                                    tileImages.set(parseInt(tileIndex), tileImg);
                                }}
                            }}
                            
                            // Update FPS counter
                            frameCount++;
                            const now = Date.now();
                            if (now - lastFpsUpdate > 1000) {{
                                const fps = frameCount / ((now - lastFpsUpdate) / 1000);
                                fpsElement.textContent = Math.round(fps);
                                frameCount = 0;
                                lastFpsUpdate = now;
                            }}
                            
                            lastFrame = Date.now();
                        }}
                    }} else if (data.type === 'info') {{
                        // Server info message
                        screenWidth = data.width;
                        screenHeight = data.height;
                        tileWidth = data.tile_width;
                        tileHeight = data.tile_height;
                        tileSize = data.tile_size;
                        totalTiles = tileWidth * tileHeight;
                        
                        // Initialize canvas size
                        canvasLayer.width = screenWidth;
                        canvasLayer.height = screenHeight;
                        
                        status.textContent = `Connected to ${{data.hostname}} (${{screenWidth}}x${{screenHeight}})`;
                        
                        // Clear tile cache
                        tileImages.clear();
                    }} else if (data.type === 'ping') {{
                        // Response to ping
                        const pingTime = Date.now() - lastPingTime;
                        latency = pingTime;
                        latencyElement.textContent = pingTime;
                        
                        // Send network stats to the server
                        sendNetworkStats();
                    }} else if (data.type === 'quality') {{
                        // Quality update from server
                        qualityLevel = data.value;
                        qualityElement.textContent = qualityLevel;
                    }} else if (data.type === 'audio') {{
                        // Handle audio data if enabled
                        if (config.audio && !config.mute) {{
                            // Audio handling would go here using WebRTC
                        }}
                    }}
                }} catch (e) {{
                    console.error('Error processing message:', e);
                }}
            }};
        }}
        
        // Send ping to measure latency
        function sendPing() {{
            if (connected && ws.readyState === WebSocket.OPEN) {{
                lastPingTime = Date.now();
                ws.send(JSON.stringify({{ type: 'ping' }}));
            }}
        }}
        
        // Send network statistics
        function sendNetworkStats() {{
            if (connected && ws.readyState === WebSocket.OPEN) {{
                // Calculate estimated bandwidth based on recent frames
                // This is a very rough estimate
                const bandwidth = 0.5; // Mbps, placeholder
                
                ws.send(JSON.stringify({{
                    type: 'network_stats',
                    latency,
                    bandwidth,
                    packet_loss: 0 // We don't have a good way to measure this in the browser
                }}));
            }}
        }}

        // Input handling
        function setupInputHandlers() {{
            const screenElem = document.getElementById('screen');
            
            // Mouse events
            screen.addEventListener('mousedown', (e) => {{
                if (!connected) return;
                
                const rect = screen.getBoundingClientRect();
                const scaleX = screenWidth / rect.width;
                const scaleY = screenHeight / rect.height;
                
                const x = Math.floor((e.clientX - rect.left) * scaleX);
                const y = Math.floor((e.clientY - rect.top) * scaleY);
                
                let button = 'left';
                if (e.button === 1) button = 'middle';
                if (e.button === 2) button = 'right';
                
                sendInputEvent({{
                    type: 'mousedown',
                    button,
                    x,
                    y
                }});
                
                e.preventDefault();
            }});
            
            screen.addEventListener('mouseup', (e) => {{
                if (!connected) return;
                
                const rect = screen.getBoundingClientRect();
                const scaleX = screenWidth / rect.width;
                const scaleY = screenHeight / rect.height;
                
                const x = Math.floor((e.clientX - rect.left) * scaleX);
                const y = Math.floor((e.clientY - rect.top) * scaleY);
                
                let button = 'left';
                if (e.button === 1) button = 'middle';
                if (e.button === 2) button = 'right';
                
                sendInputEvent({{
                    type: 'mouseup',
                    button,
                    x,
                    y
                }});
                
                e.preventDefault();
            }});
            
            screen.addEventListener('mousemove', (e) => {{
                if (!connected) return;
                
                const rect = screen.getBoundingClientRect();
                const scaleX = screenWidth / rect.width;
                const scaleY = screenHeight / rect.height;
                
                const x = Math.floor((e.clientX - rect.left) * scaleX);
                const y = Math.floor((e.clientY - rect.top) * scaleY);
                
                sendInputEvent({{
                    type: 'mousemove',
                    x,
                    y
                }});
            }});
            
            screen.addEventListener('wheel', (e) => {{
                if (!connected) return;
                
                sendInputEvent({{
                    type: 'wheel',
                    delta_y: e.deltaY
                }});
                
                e.preventDefault();
            }});
            
            // Prevent context menu
            screen.addEventListener('contextmenu', (e) => {{
                e.preventDefault();
            }});
            
            // Keyboard events
            document.addEventListener('keydown', (e) => {{
                if (!connected) return;
                
                // Don't capture browser shortcuts
                if (e.ctrlKey && (e.key === 'r' || e.key === 'F5' || e.key === 'w')) return;
                
                const modifiers = [];
                if (e.ctrlKey) modifiers.push('Control');
                if (e.altKey) modifiers.push('Alt');
                if (e.shiftKey) modifiers.push('Shift');
                if (e.metaKey) modifiers.push('Meta');
                
                sendInputEvent({{
                    type: 'keydown',
                    key: e.key,
                    modifiers
                }});
                
                // Prevent default for most keys when focused on remote screen
                if (document.activeElement === screen || screen.contains(document.activeElement)) {{
                    e.preventDefault();
                }}
            }});
            
            document.addEventListener('keyup', (e) => {{
                if (!connected) return;
                
                const modifiers = [];
                if (e.ctrlKey) modifiers.push('Control');
                if (e.altKey) modifiers.push('Alt');
                if (e.shiftKey) modifiers.push('Shift');
                if (e.metaKey) modifiers.push('Meta');
                
                sendInputEvent({{
                    type: 'keyup',
                    key: e.key,
                    modifiers
                }});
                
                // Prevent default for most keys when focused on remote screen
                if (document.activeElement === screen || screen.contains(document.activeElement)) {{
                    e.preventDefault();
                }}
            }});
        }}

        // Send input event to server
        function sendInputEvent(event) {{
            if (connected && ws.readyState === WebSocket.OPEN) {{
                ws.send(JSON.stringify(event));
            }}
        }}

        // Fullscreen toggle
        fullscreenBtn.addEventListener('click', () => {{
            if (!document.fullscreenElement) {{
                document.documentElement.requestFullscreen().catch(err => {{
                    console.error(`Error attempting to enable fullscreen: ${{err.message}}`);
                }});
            }} else {{
                document.exitFullscreen();
            }}
        }});

        // Disconnect button
        disconnectBtn.addEventListener('click', () => {{
            if (connected) {{
                ws.close();
            }}
            window.location.href = '/';
        }});

        // WebRTC for audio/video (simplified implementation)
        async function setupWebRTC() {{
            if (!config.audio) return;
            
            try {{
                // WebRTC code would go here
                console.log('WebRTC audio streaming not implemented yet');
            }} catch (err) {{
                console.error('WebRTC setup failed:', err);
            }}
        }}

        // Start the connection
        connect();
        setupInputHandlers();
        setupWebRTC();
    </script>
</body>
</html>
"#,
        stretch = stretch.to_string(),
        mute = mute.to_string(),
        audio = audio.to_string(),
        remote_only = remote_only.to_string(),
        encryption = encryption.to_string(),
        fit_mode = if stretch { "contain" } else { "scale-down" },
        display_mode = if remote_only { "position: absolute;" } else { "" },
        mute_attr = if mute { "muted" } else { "" },
        toolbar_class = if remote_only { "hidden" } else { "" }
    );

    Html(html)
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket_wrapper(socket))
}

// This wrapper function helps make our future Send by moving the non-Send ScreenCapture
// into a separate async block that will be executed by this wrapper
async fn handle_socket_wrapper(socket: WebSocket) {
    handle_socket(socket).await;
}

async fn handle_socket(socket: WebSocket) {
    // Create a thread-safe channel for screen data
    let (screen_tx, mut screen_rx) = mpsc::channel::<Result<Vec<u8>, String>>(10);
    let (delta_tx, mut delta_rx) = mpsc::channel::<Result<HashMap<usize, Vec<u8>>, String>>(10);
    
    // Setup screen capture in a separate thread
    let screen_handle = std::thread::spawn(move || {
        // This is now on a separate thread, so it doesn't need to be Send
        let mut screen_capturer = match ScreenCapture::new() {
            Ok(capturer) => capturer,
            Err(e) => {
                let err_msg = format!("Failed to initialize screen capture: {}", e);
                let _ = screen_tx.blocking_send(Err(err_msg));
                return;
            }
        };
        
        // Get screen dimensions for initial info
        let (width, height) = screen_capturer.dimensions();
        let (tile_width, tile_height, tile_size) = screen_capturer.tile_dimensions();
        
        // Send the dimensions
        let dimensions = (width, height, tile_width, tile_height, tile_size);
        let _ = screen_tx.blocking_send(Ok(bincode::serialize(&dimensions).unwrap_or_default()));
        
        // Default quality
        let current_quality = DEFAULT_QUALITY;
        let use_delta = true;
        
        // Capture loop
        loop {
            if use_delta {
                match screen_capturer.capture_jpeg_delta(Some(current_quality)) {
                    Ok(tiles) => {
                        if let Err(_) = delta_tx.blocking_send(Ok(tiles)) {
                            break;
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("Failed to capture screen: {}", e);
                        let _ = delta_tx.blocking_send(Err(err_msg));
                        break;
                    }
                }
            } else {
                match screen_capturer.capture_jpeg(current_quality) {
                    Ok(jpeg_data) => {
                        if let Err(_) = screen_tx.blocking_send(Ok(jpeg_data)) {
                            break;
                        }
                    }
                    Err(e) => {
                        let err_msg = format!("Failed to capture screen: {}", e);
                        let _ = screen_tx.blocking_send(Err(err_msg));
                        break;
                    }
                }
            }
            
            // Frame rate control
            std::thread::sleep(Duration::from_millis(1000 / 30));
        }
    });
    
    // Setup input handler
    let input_handler = Arc::new(Mutex::new(InputHandler::new()));
    
    // Get hostname
    let hostname = std::env::var("HOSTNAME")
        .unwrap_or_else(|_| "Unknown".to_string());
    
    // Setup encryption (optional)
    let encryption_key = format!("clever-kvm-{}", Uuid::new_v4());
    let _encryption_manager = Arc::new(EncryptionManager::new(&encryption_key));
    
    // Get the initial dimensions from the capture thread
    let dimensions = match screen_rx.recv().await {
        Some(Ok(data)) => {
            match bincode::deserialize::<(usize, usize, usize, usize, usize)>(&data) {
                Ok(dims) => dims,
                Err(_) => {
                    log::error!("Failed to deserialize screen dimensions");
                    return;
                }
            }
        },
        Some(Err(e)) => {
            log::error!("{}", e);
            return;
        },
        None => {
            log::error!("Screen capture thread terminated unexpectedly");
            return;
        }
    };
    
    let (width, height, tile_width, tile_height, tile_size) = dimensions;
    
    // Send initial server info
    let server_info = serde_json::json!({
        "type": "info",
        "width": width,
        "height": height,
        "hostname": hostname,
        "tile_width": tile_width,
        "tile_height": tile_height,
        "tile_size": tile_size
    });
    
    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();
    
    if let Err(e) = sender.send(Message::Text(server_info.to_string())).await {
        log::error!("Failed to send server info: {}", e);
        return;
    }
    
    // Create a channel for input events
    let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(100);
    
    // Create a channel for network stats
    let (net_stats_tx, mut net_stats_rx) = mpsc::channel::<NetworkStats>(10);
    
    // Channels for messaging
    let (message_tx, mut message_rx) = mpsc::channel::<String>(100);
    
    // Spawn a task to handle messaging
    // Instead of cloning sender, use a channel to communicate
    tokio::spawn(async move {
        while let Some(msg) = message_rx.recv().await {
            if let Err(e) = sender.send(Message::Text(msg)).await {
                log::error!("Failed to send message: {}", e);
                break;
            }
        }
    });
    
    // Spawn a task to handle incoming messages
    let input_handler_clone = input_handler.clone();
    let message_tx_clone = message_tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Try to parse as an input event
                    if let Ok(event) = serde_json::from_str::<InputEvent>(&text) {
                        if let Err(e) = input_tx.send(event).await {
                            log::error!("Failed to send input event to handler: {}", e);
                            break;
                        }
                    } else if let Ok(ping) = serde_json::from_str::<serde_json::Value>(&text) {
                        // Handle ping message
                        if ping.get("type").and_then(|t| t.as_str()) == Some("ping") {
                            let response = serde_json::json!({
                                "type": "ping",
                                "timestamp": SystemTime::now()
                                    .duration_since(SystemTime::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis()
                            });
                            
                            if let Err(e) = message_tx_clone.send(response.to_string()).await {
                                log::error!("Failed to send ping response: {}", e);
                                break;
                            }
                        }
                    } else if let Ok(stats) = serde_json::from_str::<serde_json::Value>(&text) {
                        // Handle network stats
                        if stats.get("type").and_then(|t| t.as_str()) == Some("network_stats") {
                            let latency = stats.get("latency").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
                            let bandwidth = stats.get("bandwidth").and_then(|b| b.as_f64()).unwrap_or(0.0) as f32;
                            let packet_loss = stats.get("packet_loss").and_then(|p| p.as_f64()).unwrap_or(0.0) as f32;
                            
                            if let Err(e) = net_stats_tx.send(NetworkStats {
                                latency,
                                bandwidth,
                                packet_loss,
                            }).await {
                                log::error!("Failed to send network stats: {}", e);
                            }
                            
                            // Adaptive quality based on network conditions
                            let mut new_quality = DEFAULT_QUALITY;
                            
                            if latency > 200 {
                                // High latency, reduce quality
                                new_quality = (DEFAULT_QUALITY as i32 - 10).max(MIN_QUALITY as i32) as u8;
                            } else if latency < 50 && bandwidth > 2.0 {
                                // Good conditions, increase quality
                                new_quality = (DEFAULT_QUALITY as i32 + 5).min(MAX_QUALITY as i32) as u8;
                            }
                            
                            // Send quality update
                            let quality_update = serde_json::json!({
                                "type": "quality",
                                "value": new_quality
                            });
                            
                            if let Err(e) = message_tx_clone.send(quality_update.to_string()).await {
                                log::error!("Failed to send quality update: {}", e);
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    log::error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });
    
    // Process input events
    tokio::spawn(async move {
        while let Some(event) = input_rx.recv().await {
            let mut handler = input_handler_clone.lock().unwrap();
            if let Err(e) = handler.handle_event(event) {
                log::error!("Failed to handle input event: {}", e);
            }
        }
    });
    
    // Network quality monitoring
    tokio::spawn(async move {
        let mut interval = time::interval(NETWORK_PERFORMANCE_CHECK_INTERVAL);
        
        loop {
            interval.tick().await;
            // Process any available network stats
            while let Ok(stats) = net_stats_rx.try_recv() {
                // Adjust quality based on network conditions
                let mut new_quality = DEFAULT_QUALITY;
                
                if stats.latency > 200 || stats.packet_loss > 5.0 {
                    // Bad network conditions, reduce quality
                    new_quality = (DEFAULT_QUALITY as i32 - 10).max(MIN_QUALITY as i32) as u8;
                } else if stats.latency < 50 && stats.bandwidth > 2.0 && stats.packet_loss < 1.0 {
                    // Good network conditions, increase quality
                    new_quality = (DEFAULT_QUALITY as i32 + 5).min(MAX_QUALITY as i32) as u8;
                }
                
                // In a real implementation, we'd send this to the screen capturer
                log::debug!("Adaptive quality: {}", new_quality);
            }
        }
    });
    
    // Main message loop - use a separate channel to communicate with the client
    let message_tx_for_frames = message_tx.clone();
    
    // Frame rate control
    let target_fps = 30;
    let frame_interval = Duration::from_millis(1000 / target_fps);
    let mut last_frame = Instant::now();
    
    // Main loop for sending frames
    let use_delta = true;
    
    loop {
        if use_delta {
            // Handle delta frames
            match delta_rx.recv().await {
                Some(Ok(tiles)) => {
                    if !tiles.is_empty() {
                        // Convert each tile to base64
                        let mut base64_tiles = HashMap::new();
                        
                        for (idx, jpeg_data) in tiles {
                            // Apply compression if needed for large tiles
                            let processed_data = if jpeg_data.len() > 8192 {
                                match compress_data(&jpeg_data, 3) {
                                    Ok(compressed) => compressed,
                                    Err(_) => jpeg_data,
                                }
                            } else {
                                jpeg_data
                            };
                            
                            base64_tiles.insert(idx, general_purpose::STANDARD.encode(&processed_data));
                        }
                        
                        // Create delta frame data
                        let delta_data = serde_json::json!({
                            "type": "delta",
                            "tiles": base64_tiles,
                            "timestamp": SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis()
                        });
                        
                        // Send the delta frame through the message channel
                        if let Err(e) = message_tx_for_frames.send(delta_data.to_string()).await {
                            log::error!("Failed to send delta frame: {}", e);
                            break;
                        }
                    }
                }
                Some(Err(e)) => {
                    log::error!("{}", e);
                    break;
                }
                None => {
                    log::error!("Delta capture channel closed");
                    break;
                }
            }
        } else {
            // Handle normal frames
            match screen_rx.recv().await {
                Some(Ok(jpeg_data)) => {
                    // Encode to base64
                    let base64_image = general_purpose::STANDARD.encode(&jpeg_data);
                    
                    // Create frame data
                    let frame_data = serde_json::json!({
                        "type": "frame",
                        "width": width,
                        "height": height,
                        "image": base64_image,
                        "timestamp": SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    });
                    
                    // Send the frame through the message channel
                    if let Err(e) = message_tx_for_frames.send(frame_data.to_string()).await {
                        log::error!("Failed to send frame: {}", e);
                        break;
                    }
                }
                Some(Err(e)) => {
                    log::error!("{}", e);
                    break;
                }
                None => {
                    log::error!("Screen capture channel closed");
                    break;
                }
            }
        }
        
        // Sleep to maintain target frame rate
        let elapsed = last_frame.elapsed();
        if elapsed < frame_interval {
            time::sleep(frame_interval - elapsed).await;
        }
        last_frame = Instant::now();
    }
    
    // Wait for screen capture thread to finish
    let _ = screen_handle.join();
}