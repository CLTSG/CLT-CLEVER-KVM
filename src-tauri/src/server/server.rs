use axum::{
    routing::get,
    Router,
    handler::HandlerWithoutStateExt,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::{
    sync::{mpsc, broadcast},
    task::JoinHandle,
    net::TcpListener
};
use tauri::AppHandle;
use tower_http::trace::TraceLayer;
use tower_http::services::ServeDir;
use std::convert::Infallible;
use axum::http::{StatusCode, Response};
use axum::body::Body;

use super::handlers::{kvm_client_handler, static_file_handler, ws_handler_with_stop};

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

pub struct WebSocketServer {
    shutdown_tx: mpsc::Sender<()>,
    server_handle: JoinHandle<()>,
    // Add broadcast channel for signaling all connections to stop
    stop_broadcast: broadcast::Sender<()>,
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
        
        // Broadcast channel for stopping all connections
        let (stop_broadcast, _) = broadcast::channel::<()>(10);
        let stop_broadcast_clone = stop_broadcast.clone();
        
        // Get the correct web-client path
        let web_client_path = get_web_client_path();
        log::info!("Using web-client directory: {:?}", web_client_path);
        
        // Set up the router
        let app = Router::new()
            .route("/ws", get(move |ws: axum::extract::ws::WebSocketUpgrade, query: axum::extract::Query<std::collections::HashMap<String, String>>| {
                let stop_rx = stop_broadcast_clone.subscribe();
                async move { ws_handler_with_stop(ws, query, stop_rx).await }
            }))
            .route("/kvm", get(kvm_client_handler))
            .route("/static/*path", get(static_file_handler))
            .fallback_service(
                ServeDir::new(&web_client_path)
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
            stop_broadcast,
        })
    }

    pub async fn shutdown(self) {
        // Signal all connections to stop
        let _ = self.stop_broadcast.send(());
        
        // Give connections a moment to clean up
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
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
