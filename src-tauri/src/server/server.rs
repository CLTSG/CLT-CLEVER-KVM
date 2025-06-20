use axum::{
    routing::get,
    Router,
    handler::HandlerWithoutStateExt,
};
use std::net::SocketAddr;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    net::TcpListener
};
use tauri::AppHandle;
use tower_http::trace::TraceLayer;
use tower_http::services::ServeDir;
use std::convert::Infallible;
use axum::http::{StatusCode, Response};
use axum::body::Body;

use super::handlers::{ws_handler, kvm_client_handler, static_file_handler};

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
            .route("/static/*path", get(static_file_handler))
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
