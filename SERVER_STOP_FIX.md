# Fix for Video Encoding Continuing After Server Stop

## Problem Description

When clicking the "Stop Server" button in the Server Status component, the video encoding process continued running in the terminal even though the web server was stopped. This happened because the WebSocket connections and their associated background threads (particularly the screen capture threads) were not being properly terminated when the server was shut down.

## Root Cause

The issue was in the server shutdown process:

1. The `stop_server` Tauri command only called `server.shutdown().await` on the HTTP server
2. The `WebSocketServer::shutdown()` method only stopped the HTTP listener but didn't signal active WebSocket connections to stop
3. WebSocket connections spawned background threads for screen capture that ran in infinite loops without any stop mechanism
4. These threads continued capturing and encoding video even after the server was "stopped"

## Solution Implementation

The fix involved implementing a proper shutdown mechanism with broadcast signals:

### 1. Enhanced Server Structure

Modified `WebSocketServer` to include a broadcast channel for signaling all connections to stop:

```rust
pub struct WebSocketServer {
    shutdown_tx: mpsc::Sender<()>,
    server_handle: JoinHandle<()>,
    stop_broadcast: broadcast::Sender<()>, // New: broadcast channel for stopping all connections
}
```

### 2. Updated WebSocket Handler

Created a new WebSocket handler that accepts a stop signal:

- `ws_handler_with_stop()` - New handler that receives a broadcast receiver
- `handle_socket_wrapper_with_stop()` - Wrapper function that includes the stop signal
- `handle_legacy_socket_with_stop()` - Main socket handler with proper cleanup

### 3. Stop Signal Distribution

The server now creates multiple receivers from the broadcast channel and distributes them to:

- Client message handler (for WebSocket messages)
- Frame handler (for video/audio streaming)
- Main connection lifecycle

### 4. Screen Capture Thread Termination

Enhanced the screen capture thread with:

- A dedicated stop signal channel (`thread_stop_tx`/`thread_stop_rx`)
- Loop condition checking for stop signal: `if thread_stop_rx.try_recv().is_ok() { break; }`
- Proper thread cleanup when the server stops

### 5. Graceful Shutdown Process

The new shutdown process:

1. Broadcasts stop signal to all active connections
2. Waits 500ms for connections to clean up
3. Stops the HTTP server
4. Waits for all tasks to complete
5. Ensures screen capture threads are properly terminated

## Code Changes

### Files Modified:

1. **`src-tauri/src/server/server.rs`**
   - Added broadcast channel for stop signals
   - Updated router to use new WebSocket handler
   - Enhanced shutdown method with connection cleanup

2. **`src-tauri/src/server/handlers.rs`**
   - Added `ws_handler_with_stop()` function
   - Updated imports for broadcast channel support

3. **`src-tauri/src/server/websocket.rs`**
   - Added `handle_socket_wrapper_with_stop()` and related functions
   - Implemented `handle_legacy_socket_with_stop()` with proper cleanup
   - Added stop signal checks in all async loops
   - Enhanced screen capture thread with termination signal

## Testing the Fix

To test that the fix works:

1. Start the KVM server
2. Connect a client (open the KVM URL)
3. Observe video encoding activity in the terminal
4. Click "Stop Server" button
5. Verify that video encoding stops immediately and the screen capture thread exits

## Benefits

- **Proper Resource Cleanup**: All background threads are now properly terminated
- **Immediate Response**: Server stop is now immediate and complete
- **Memory Safety**: No more orphaned threads consuming CPU and memory
- **Better User Experience**: Clear indication when the server is truly stopped

## Technical Details

The solution uses Tokio's broadcast channel which allows:
- One sender to signal multiple receivers
- Non-blocking checks with `try_recv()`
- Proper async/await integration
- Graceful degradation if receivers are dropped

This ensures that when the user clicks "Stop Server", all video encoding processes cease immediately and cleanly.
