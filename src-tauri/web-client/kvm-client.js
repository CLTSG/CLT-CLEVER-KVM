class KVMClient {
    constructor(config) {
        this.config = config;
        this.ws = null;
        this.connected = false;
        this.lastFrame = 0;
        this.frameCount = 0;
        this.lastFpsUpdate = Date.now();
        this.screenWidth = 0;
        this.screenHeight = 0;
        this.latency = 0;
        this.lastPingTime = 0;
        this.pingInterval = null;
        this.qualityLevel = 85;
        this.availableMonitors = [];
        this.currentMonitor = config.monitor;
        this.currentCodec = config.codec;
        this.mediaSource = null;
        this.sourceBuffer = null;
        this.videoQueue = [];
        this.showStats = false;
        
        // Rendering mode and canvas properties
        this.renderingMode = 'video'; // 'video' or 'canvas'
        this.canvas = null;
        this.ctx = null;
        this.playButton = null;
        this.needsKeyframe = true;
        
        // OSD state
        this.osdVisible = true;
        this.osdTimer = null;
        this.mouseIdleTimer = null;
        this.lastMouseMove = Date.now();
        
        // Multi-touch and gesture support
        this.touchIdentifiers = new Map();
        this.gestureInProgress = false;
        this.initialTouchDistance = 0;
        this.initialTouchAngle = 0;

        // WebRTC for audio
        this.peerConnection = null;
        this.audioStream = null;

        // Add MediaSource failure tracking
        this.mediaSourceFailures = 0;
        this.maxMediaSourceFailures = 3; // Switch to canvas after 3 failures
        this.lastFailureTime = 0;
        this.failureResetTime = 30000; // Reset failure count after 30 seconds

        this.initializeElements();
        this.setupEventListeners();
        this.connect();
    }

    initializeElements() {
        // Main elements - only use video element for H.264
        this.videoScreen = document.getElementById('video-screen');
        this.canvasLayer = document.getElementById('canvas-layer');
        if (this.canvasLayer) {
            this.ctx = this.canvasLayer.getContext('2d');
        }
        this.audioElement = document.getElementById('remote-audio');
        
        // OSD elements
        this.osdOverlay = document.querySelector('.osd-overlay');
        this.statusDisplay = document.querySelector('.status-display');
        this.osdTitle = document.querySelector('.osd-title');
        this.networkStats = document.querySelector('.network-stats');
        this.gestureIndicator = document.querySelector('.gesture-indicator');
        this.notificationArea = document.querySelector('.notification-area');
        
        // Controls
        this.monitorDropdown = document.getElementById('monitor-dropdown');
        this.codecDropdown = document.getElementById('codec-dropdown');
        this.qualityDropdown = document.getElementById('quality-dropdown');
        this.settingsPanel = document.querySelector('.settings-panel');
        
        // WebRTC quality tracking
        this.currentQuality = 'medium';
        this.adaptiveQuality = true;
        this.networkStats = {
            bandwidth: 0,
            latency: 0,
            packetLoss: 0
        };
        this.frameStats = {
            framesReceived: 0,
            keyframesReceived: 0,
            totalBytes: 0,
            currentFps: 0,
            lastFrameCount: 0,
            lastFpsUpdate: Date.now()
        };
        
        // Settings controls - check if they exist before using
        this.settingStretch = document.getElementById('setting-stretch');
        this.settingAudio = document.getElementById('setting-audio');
        this.settingMute = document.getElementById('setting-mute');
        this.settingStats = document.getElementById('setting-stats');
        
        // Fix: Use bitrate-slider instead of quality-slider
        this.bitrateSlider = document.getElementById('bitrate-slider');
        this.bitrateValue = document.getElementById('bitrate-value');

        // Initialize settings from config - only if elements exist
        if (this.settingStretch) this.settingStretch.checked = this.config.stretch;
        if (this.settingAudio) this.settingAudio.checked = this.config.audio;
        if (this.settingMute) this.settingMute.checked = this.config.mute;
        if (this.codecDropdown) this.codecDropdown.value = this.config.codec;
        
        if (this.audioElement) this.audioElement.muted = this.config.mute;
        
        // Ensure video element is visible
        if (this.videoScreen) {
            this.videoScreen.style.display = 'block';
        }
    }

    setupEventListeners() {
        // OSD auto-hide functionality
        document.addEventListener('mousemove', (e) => {
            this.handleMouseActivity();
        });

        document.addEventListener('click', () => {
            this.handleMouseActivity();
        });

        document.addEventListener('keydown', (e) => {
            this.handleMouseActivity();
        });

        // Screen interactions
        this.setupInputHandlers();
        
        // Control buttons - check if they exist
        const fullscreenBtn = document.getElementById('fullscreen-btn');
        if (fullscreenBtn) {
            fullscreenBtn.addEventListener('click', () => {
                this.toggleFullscreen();
            });
        }

        const settingsBtn = document.getElementById('settings-btn');
        if (settingsBtn) {
            settingsBtn.addEventListener('click', () => {
                this.toggleSettings();
            });
        }

        const disconnectBtn = document.getElementById('disconnect-btn');
        if (disconnectBtn) {
            disconnectBtn.addEventListener('click', () => {
                this.disconnect();
            });
        }

        // Settings panel
        const settingsSave = document.getElementById('settings-save');
        if (settingsSave) {
            settingsSave.addEventListener('click', () => {
                this.saveSettings();
            });
        }

        const settingsCancel = document.getElementById('settings-cancel');
        if (settingsCancel) {
            settingsCancel.addEventListener('click', () => {
                this.hideSettings();
            });
        }

        const closeButton = document.querySelector('.close-button');
        if (closeButton) {
            closeButton.addEventListener('click', () => {
                this.hideSettings();
            });
        }

        // Settings controls - only add listeners if elements exist
        if (this.bitrateSlider && this.bitrateValue) {
            this.bitrateSlider.addEventListener('input', (e) => {
                this.bitrateValue.textContent = e.target.value;
            });
        }

        if (this.settingStats) {
            this.settingStats.addEventListener('change', (e) => {
                this.showStats = e.target.checked;
                this.updateStatsVisibility();
            });
        }

        // Monitor and codec selection
        if (this.monitorDropdown) {
            this.monitorDropdown.addEventListener('change', (e) => {
                const newMonitor = parseInt(e.target.value);
                if (newMonitor !== this.currentMonitor) {
                    this.switchMonitor(newMonitor);
                }
            });
        }

        // Codec dropdown is no longer needed - using WebRTC H.264 only

        if (this.qualityDropdown) {
            this.qualityDropdown.addEventListener('change', (e) => {
                const selectedQuality = e.target.value;
                if (selectedQuality === 'auto') {
                    this.adaptiveQuality = true;
                    this.showNotification('Auto quality enabled', 2000);
                } else {
                    this.adaptiveQuality = false;
                    this.switchQuality(selectedQuality);
                }
            });
        }

        // Click outside settings to close
        document.addEventListener('click', (e) => {
            if (this.settingsPanel && this.settingsPanel.classList.contains('visible') && 
                !this.settingsPanel.contains(e.target) && 
                !document.getElementById('settings-btn')?.contains(e.target)) {
                this.hideSettings();
            }
        });

        // Keyboard shortcuts
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') {
                if (this.settingsPanel && this.settingsPanel.classList.contains('visible')) {
                    this.hideSettings();
                } else if (document.fullscreenElement) {
                    document.exitFullscreen();
                }
            } else if (e.key === 'F11') {
                e.preventDefault();
                this.toggleFullscreen();
            } else if (e.key === 's' && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                this.toggleSettings();
            }
        });
    }

    handleMouseActivity() {
        this.lastMouseMove = Date.now();
        this.showOSD();
        
        // Clear existing timer
        if (this.mouseIdleTimer) {
            clearTimeout(this.mouseIdleTimer);
        }
        
        // Set timer to hide OSD after 3 seconds of inactivity
        this.mouseIdleTimer = setTimeout(() => {
            this.hideOSD();
        }, 3000);
    }

    showOSD() {
        if (this.osdOverlay) {
            this.osdVisible = true;
            this.osdOverlay.classList.add('visible');
        }
        const screenElement = document.getElementById('screen');
        if (screenElement) {
            screenElement.classList.add('show-cursor');
        }
    }

    hideOSD() {
        if (this.settingsPanel && this.settingsPanel.classList.contains('visible')) {
            return; // Don't hide OSD while settings are open
        }
        
        if (this.osdOverlay) {
            this.osdVisible = false;
            this.osdOverlay.classList.remove('visible');
        }
        const screenElement = document.getElementById('screen');
        if (screenElement) {
            screenElement.classList.remove('show-cursor');
        }
    }

    // Input handling methods
    setupInputHandlers() {
        const screenContainer = document.getElementById('screen');
        
        // Mouse events - use video element for all codecs
        ['mousedown', 'mouseup', 'mousemove', 'wheel'].forEach(event => {
            if (this.videoScreen) {
                this.videoScreen.addEventListener(event, (e) => this.handleMouseEvent(e));
            }
        });
        
        // Touch events
        ['touchstart', 'touchmove', 'touchend', 'touchcancel'].forEach(event => {
            if (screenContainer) {
                screenContainer.addEventListener(event, (e) => this.handleTouchEvent(e), { passive: false });
            }
        });
        
        // Keyboard events
        document.addEventListener('keydown', (e) => this.handleKeyEvent(e, 'keydown'));
        document.addEventListener('keyup', (e) => this.handleKeyEvent(e, 'keyup'));
        
        // Prevent context menu on video screen
        if (this.videoScreen) {
            this.videoScreen.addEventListener('contextmenu', (e) => e.preventDefault());
        }
    }

    handleMouseEvent(e) {
        if (!this.connected) return;
        
        // Use video screen for mouse events
        if (!this.videoScreen) return;
        
        const rect = this.videoScreen.getBoundingClientRect();
        const scaleX = this.screenWidth / rect.width;
        const scaleY = this.screenHeight / rect.height;
        
        const x = Math.floor((e.clientX - rect.left) * scaleX);
        const y = Math.floor((e.clientY - rect.top) * scaleY);
        
        let eventData = {
            x,
            y,
            monitor_id: this.getActiveMonitorId()
        };
        
        switch(e.type) {
            case 'mousedown':
                eventData.type = 'mousedown';
                eventData.button = e.button === 0 ? 'left' : (e.button === 1 ? 'middle' : 'right');
                this.sendInputEvent(eventData);
                break;
            case 'mouseup':
                eventData.type = 'mouseup';
                eventData.button = e.button === 0 ? 'left' : (e.button === 1 ? 'middle' : 'right');
                this.sendInputEvent(eventData);
                break;
            case 'mousemove':
                eventData.type = 'mousemove';
                this.sendInputEvent(eventData);
                break;
            case 'wheel':
                e.preventDefault();
                eventData.type = 'wheel';
                eventData.delta_y = e.deltaY;
                eventData.delta_x = e.deltaX;
                this.sendInputEvent(eventData);
                break;
        }
    }

    handleKeyEvent(e, type) {
        if (!this.connected) return;
        
        // Don't capture certain keys if settings panel is open
        if (this.settingsPanel && this.settingsPanel.classList.contains('visible')) {
            return;
        }
        
        // Let some special keys pass through
        if (['F11', 'F12'].includes(e.key) || 
            (e.key === 's' && (e.ctrlKey || e.metaKey))) {
            return;
        }
        
        e.preventDefault();
        
        this.sendInputEvent({
            type: type,
            key: e.key,
            keyCode: e.keyCode,
            ctrlKey: e.ctrlKey,
            altKey: e.altKey,
            shiftKey: e.shiftKey,
            metaKey: e.metaKey,
            monitor_id: this.getActiveMonitorId()
        });
    }

    handleTouchEvent(e) {
        e.preventDefault();
        
        if (!this.connected) return;
        
        // Handle touch events for mobile devices
        for (let i = 0; i < e.changedTouches.length; i++) {
            const touch = e.changedTouches[i];
            const rect = this.videoScreen.getBoundingClientRect();
            const scaleX = this.screenWidth / rect.width;
            const scaleY = this.screenHeight / rect.height;
            
            const x = Math.floor((touch.clientX - rect.left) * scaleX);
            const y = Math.floor((touch.clientY - rect.top) * scaleY);
            
            let eventData = {
                type: e.type,
                x,
                y,
                identifier: touch.identifier,
                monitor_id: this.getActiveMonitorId()
            };
            
            this.sendInputEvent(eventData);
        }
    }

    sendInputEvent(event) {
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(event));
        }
    }

    sendMessage(message) {
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(message));
        }
    }

    getActiveMonitorId() {
        return this.availableMonitors[this.currentMonitor]?.id || 'primary';
    }

    // Utility methods
    sendPing() {
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.lastPingTime = Date.now();
            this.ws.send(JSON.stringify({
                type: 'ping',
                timestamp: this.lastPingTime
            }));
        }
    }

    handlePingResponse() {
        if (this.lastPingTime > 0) {
            this.latency = Date.now() - this.lastPingTime;
            const latencyElement = document.getElementById('latency');
            if (latencyElement) {
                latencyElement.textContent = this.latency;
            }
        }
    }

    sendNetworkStats() {
        const stats = {
            latency: this.latency,
            bandwidth: this.estimateBandwidth(),
            packet_loss: this.estimatePacketLoss()
        };
        
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({
                type: 'network_stats',
                stats: stats
            }));
        }
    }

    estimateBandwidth() {
        // Simple bandwidth estimation based on frame rate and quality
        const bytesPerFrame = (this.screenWidth * this.screenHeight * this.qualityLevel) / 1000;
        const fps = this.frameCount;
        return (bytesPerFrame * fps * 8) / 1024; // kbps
    }

    estimatePacketLoss() {
        // Simplified packet loss estimation
        return Math.max(0, (this.latency - 50) / 500);
    }

    showNotification(message, duration = 3000) {
        if (!this.notificationArea) return;
        
        const notification = document.createElement('div');
        notification.className = 'notification';
        notification.textContent = message;
        
        this.notificationArea.appendChild(notification);
        
        setTimeout(() => {
            notification.classList.add('show');
        }, 10);
        
        setTimeout(() => {
            notification.classList.remove('show');
            setTimeout(() => {
                if (notification.parentNode) {
                    notification.parentNode.removeChild(notification);
                }
            }, 300);
        }, duration);
    }

    toggleSettings() {
        if (this.settingsPanel) {
            if (this.settingsPanel.classList.contains('visible')) {
                this.hideSettings();
            } else {
                this.showSettings();
            }
        }
    }

    showSettings() {
        if (this.settingsPanel) {
            this.settingsPanel.classList.add('visible');
        }
    }

    hideSettings() {
        if (this.settingsPanel) {
            this.settingsPanel.classList.remove('visible');
        }
    }

    updateStatsVisibility() {
        if (this.networkStats) {
            this.networkStats.style.display = this.showStats ? 'block' : 'none';
        }
    }

    updateFrameStats() {
        this.frameCount++;
        const now = Date.now();
        
        if (now - this.lastFpsUpdate >= 1000) {
            const fps = this.frameCount;
            this.frameCount = 0;
            this.lastFpsUpdate = now;
            
            const fpsElement = document.getElementById('fps');
            if (fpsElement) {
                fpsElement.textContent = fps;
            }
        }
    }

    saveSettings() {
        // Get values from settings panel
        if (this.settingStretch) {
            this.config.stretch = this.settingStretch.checked;
        }
        if (this.settingAudio) {
            this.config.audio = this.settingAudio.checked;
        }
        if (this.settingMute) {
            this.config.mute = this.settingMute.checked;
            if (this.audioElement) {
                this.audioElement.muted = this.config.mute;
            }
        }
        
        // Apply stretch setting to video
        if (this.videoScreen) {
            if (this.config.stretch) {
                this.videoScreen.style.width = '100%';
                this.videoScreen.style.height = '100%';
                this.videoScreen.style.objectFit = 'fill';
            } else {
                this.videoScreen.style.width = 'auto';
                this.videoScreen.style.height = 'auto';
                this.videoScreen.style.objectFit = 'contain';
            }
        }
        
        this.hideSettings();
        this.showNotification('Settings saved');
    }

    toggleFullscreen() {
        if (!document.fullscreenElement) {
            document.documentElement.requestFullscreen().catch(err => {
                console.error(`Error attempting to enable fullscreen: ${err.message}`);
                this.showNotification(`Fullscreen error: ${err.message}`);
            });
        } else {
            document.exitFullscreen();
        }
    }

    disconnect() {
        if (this.connected) {
            this.ws.close();
        }
        
        // Stop network monitoring
        this.stopNetworkMonitoring();
        
        window.location.href = '/';
    }

    // Monitor and codec switching
    switchMonitor(monitorIndex) {
        console.log('Switching to monitor:', monitorIndex);
        this.currentMonitor = monitorIndex;
        // Reconnect with new monitor
        if (this.ws) {
            this.ws.close();
        }
    }

    sendQualitySetting(quality) {
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({
                type: 'quality_update',
                quality: parseInt(quality)
            }));
        }
    }

    // WebRTC setup for audio
    setupWebRTC(encryption) {
        if (!this.config.audio) return;
        
        console.log('Setting up WebRTC for audio streaming');
        
        // This would be implemented for actual WebRTC audio support
        // For now, just log that it's being set up
        if (encryption) {
            console.log('WebRTC will use encryption');
        }
    }

    handleWebRTCOffer(data) {
        console.log('Received WebRTC offer:', data);
        
        // In a real implementation, this would:
        // 1. Create RTCPeerConnection
        // 2. Set remote description with the offer
        // 3. Create and send answer back to server
        
        // For now, just acknowledge
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({
                type: 'webrtc_answer',
                sdp: 'mock_answer_sdp'
            }));
        }
    }

    handleQualityUpdate(data) {
        console.log('Quality update received:', data);
        this.qualityLevel = data.quality || data.value || 85;
        
        // Update UI elements
        const qualityElements = document.querySelectorAll('#quality');
        qualityElements.forEach(el => el.textContent = this.qualityLevel);
    }

    handleMonitorList(data) {
        console.log('Monitor list received:', data);
        this.availableMonitors = data.monitors || [];
        
        // Update monitor dropdown
        if (this.monitorDropdown) {
            this.monitorDropdown.innerHTML = '';
            
            if (this.availableMonitors.length > 0) {
                this.availableMonitors.forEach((monitor, index) => {
                    const option = document.createElement('option');
                    option.value = index;
                    option.textContent = `${monitor.name} ${monitor.is_primary ? '(Primary)' : ''} - ${monitor.width}x${monitor.height}`;
                    this.monitorDropdown.appendChild(option);
                });
            } else {
                // Fallback if no monitors are detected
                const option = document.createElement('option');
                option.value = 0;
                option.textContent = 'Primary Monitor';
                this.monitorDropdown.appendChild(option);
            }
            
            // Set current selection
            this.monitorDropdown.value = this.currentMonitor;
        }
        
        // If this is the first monitor list and status is still showing, hide it
        if (this.statusDisplay && this.statusDisplay.style.display !== 'none') {
            setTimeout(() => {
                if (this.statusDisplay) {
                    this.statusDisplay.style.display = 'none';
                }
            }, 500);
        }
    }

    // Connect method
    connect() {
        this.updateStatus('Connecting', 'Establishing connection to server...', true);
        
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        
        // Get the hostname from the current URL - this should preserve IP addresses and hostnames
        let hostname = window.location.hostname;
        
        // Debug logging
        console.log('Current location:', window.location.href);
        console.log('Hostname extracted:', hostname);
        console.log('Port from location:', window.location.port);
        
        // Check for manual server override in URL parameters
        const urlParams = new URLSearchParams(window.location.search);
        const serverOverride = urlParams.get('server');
        
        // Determine the WebSocket host
        let wsHost;
        if (serverOverride) {
            // Manual server override via URL parameter: ?server=192.168.1.100:9921
            wsHost = serverOverride;
            console.log('Using server override from URL:', wsHost);
        } else if (window.location.port && window.location.port !== '80' && window.location.port !== '443') {
            // If we're on a custom port (like the Vite dev server), use the hostname with port 9921
            wsHost = `${hostname}:9921`;
        } else {
            // If we're on standard HTTP/HTTPS ports, assume KVM is also on the same host with port 9921
            wsHost = `${hostname}:9921`;
        }
        
        const wsUrl = `${protocol}//${wsHost}/ws?monitor=${this.currentMonitor}&codec=${this.currentCodec}${this.config.audio ? '&audio=true' : ''}`;
        
        console.log('Connecting to WebSocket:', wsUrl);
        console.log('WebSocket host resolved to:', wsHost);
        
        this.ws = new WebSocket(wsUrl);
        
        this.ws.onopen = () => {
            this.connected = true;
            this.updateStatus('Connected', 'Connection established successfully');
            console.log('WebSocket connection established');
            
            // Start sending ping messages to measure latency
            this.pingInterval = setInterval(() => {
                this.sendPing();
            }, 5000);

            // Start network monitoring and adaptive quality
            this.startNetworkMonitoring();
            
            // Request monitor list if not received within 2 seconds
            setTimeout(() => {
                if (this.availableMonitors.length === 0) {
                    console.log('No monitors received, using fallback...');
                    // Create a fallback monitor entry
                    this.availableMonitors = [{
                        id: "primary",
                        name: "Primary Monitor", 
                        width: this.screenWidth || 1920,
                        height: this.screenHeight || 1080,
                        is_primary: true
                    }];
                    this.handleMonitorList({ monitors: this.availableMonitors });
                }
            }, 2000);
        };
        
        this.ws.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data);
                this.handleMessage(data);
            } catch (e) {
                console.error('Error parsing WebSocket message:', e);
            }
        };
        
        this.ws.onclose = (event) => {
            this.connected = false;
            if (this.pingInterval) {
                clearInterval(this.pingInterval);
                this.pingInterval = null;
            }
            
            console.log('WebSocket closed. Code:', event.code, 'Reason:', event.reason);
            
            if (event.code === 1006) {
                this.updateStatus('Connection Failed', `Could not connect to KVM server at ${wsHost}. Please check that the server is running and accessible.`);
            } else {
                this.updateStatus('Disconnected', 'Connection closed');
            }
            
            // Only attempt to reconnect if it was a normal closure, not a connection failure
            if (event.code !== 1006) {
                setTimeout(() => {
                    if (!this.connected) {
                        console.log('Attempting to reconnect...');
                        this.connect();
                    }
                }, 3000);
            }
        };
        
        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            console.error('Failed to connect to:', wsUrl);
            this.updateStatus('Connection Error', `Failed to connect to KVM server. Check that port 9921 is accessible on ${hostname}.`);
        };
    }

    handleMessage(data) {
        switch(data.type) {
            case 'server_info':
            case 'info':  // Fallback for older message type
                this.handleServerInfo(data);
                break;
            case 'video_frame':
                this.handleVideoFrame(data);
                break;
            case 'pong':
                this.handlePingResponse();
                break;
            case 'quality_update':
                this.handleQualityUpdate(data);
                break;
            case 'monitors':
                this.handleMonitorList(data);
                break;
            case 'webrtc_offer':
                this.handleWebRTCOffer(data);
                break;
            case 'streaming_stats':
                this.handleStreamingStats(data);
                break;
            case 'webrtc_frame':
                this.handleWebRTCFrame(data);
                break;
            default:
                console.log('Unknown message type:', data.type);
        }
    }

    handleServerInfo(data) {
        console.log('Server info received:', data);
        
        this.screenWidth = data.width;
        this.screenHeight = data.height;
        
        // Update UI
        if (this.osdTitle) {
            this.osdTitle.textContent = `${data.hostname} - Monitor ${data.monitor} (${data.width}x${data.height})`;
        }
        
        // Normalize codec name - server sends "webrtc" but we use "h264" internally
        this.currentCodec = data.codec === 'webrtc' ? 'h264' : (data.codec || this.config.codec);
        console.log('Normalized codec from', data.codec, 'to', this.currentCodec);
        if (this.codecDropdown) {
            this.codecDropdown.value = 'h264'; // Always set to h264 since it's our only option
        }
        
        // Initialize canvas size
        if (this.canvasLayer) {
            this.canvasLayer.width = this.screenWidth;
            this.canvasLayer.height = this.screenHeight;
        }
        
        // Ensure video element is visible
        if (this.videoScreen) {
            this.videoScreen.style.display = 'block';
        }
        
        // Initialize video for codec streaming
        this.initializeVideoStreaming();
        
        // Initialize WebRTC for audio if enabled
        if (this.config.audio && data.audio) {
            this.setupWebRTC(data.encryption);
        }
        
        // Hide loading status after successful connection
        setTimeout(() => {
            if (this.statusDisplay) {
                this.statusDisplay.style.display = 'none';
            }
        }, 1000);
        
        this.showNotification(`Connected to ${data.hostname} - ${data.width}x${data.height} using ${data.codec}`);
    }

    initializeVideoStreaming() {
        if (!this.videoScreen) {
            console.error('Video screen element not found');
            return;
        }
        
        console.log('Initializing video streaming for codec:', this.currentCodec);
        
        // For network access, prefer canvas mode as it's more reliable
        if (this.isNetworkAccess()) {
            console.log('Network access detected, preferring canvas mode for better compatibility');
            this.switchToCanvasRendering();
            return;
        }
        
        // For H.264 via WebRTC, we need to set up the video element properly
        if (this.currentCodec === 'h264' || this.currentCodec === 'webrtc') {
            // Set video dimensions
            this.videoScreen.width = this.screenWidth;
            this.videoScreen.height = this.screenHeight;
        
            // Apply stretch setting
            if (this.config.stretch) {
                this.videoScreen.style.width = '100%';
                this.videoScreen.style.height = '100%';
                this.videoScreen.style.objectFit = 'fill';
            } else {
                this.videoScreen.style.width = 'auto';
                this.videoScreen.style.height = 'auto';
                this.videoScreen.style.objectFit = 'contain';
            }
        
            // Initialize MediaSource for H.264 if supported
            if (this.currentCodec === 'h264') {
                this.initializeMediaSource(this.currentCodec);
            }
        }
    }

    initializeMediaSource(codec) {
        console.log('Initializing MediaSource for codec:', codec);
        
        if (!window.MediaSource) {
            console.warn('MediaSource API not supported, switching to canvas rendering');
            this.switchToCanvasRendering();
            return;
        }
        
        // Test multiple codec configurations
        const codecConfigs = this.getCodecConfigurations(codec);
        let mimeType = null;
        
        console.log('Testing codec configurations:', codecConfigs);
        
        for (const config of codecConfigs) {
            console.log(`Testing MIME type: ${config}`);
            if (MediaSource.isTypeSupported(config)) {
                mimeType = config;
                console.log(`✓ Supported MIME type found: ${config}`);
                break;
            } else {
                console.log(`✗ Not supported: ${config}`);
            }
        }
        
        if (!mimeType) {
            console.warn('No supported codec configurations found, switching to canvas rendering');
            this.switchToCanvasRendering();
            return;
        }
        
        // Clean up existing MediaSource
        if (this.mediaSource) {
            try {
                if (this.sourceBuffer && this.mediaSource.readyState === 'open') {
                    this.mediaSource.removeSourceBuffer(this.sourceBuffer);
                }
                if (this.videoScreen.src && this.videoScreen.src.startsWith('blob:')) {
                    URL.revokeObjectURL(this.videoScreen.src);
                }
            } catch (e) {
                console.error('Error cleaning up MediaSource:', e);
            }
        }
        
        this.mediaSource = new MediaSource();
        this.sourceBuffer = null;
        this.videoQueue = [];
        this.needsKeyframe = true;
        
        // Create object URL and set it to video element
        const objectURL = URL.createObjectURL(this.mediaSource);
        this.videoScreen.src = objectURL;
        
        // Set up MediaSource event handlers
        this.mediaSource.addEventListener('sourceopen', () => {
            console.log('MediaSource opened, adding SourceBuffer with MIME type:', mimeType);
            try {
                if (this.mediaSource.readyState === 'open') {
                    this.sourceBuffer = this.mediaSource.addSourceBuffer(mimeType);
                    this.sourceBuffer.mode = 'sequence';
                    
                    // Set up SourceBuffer event handlers
                    this.sourceBuffer.addEventListener('updateend', () => {
                        // Process queued video data
                        if (this.videoQueue.length > 0 && !this.sourceBuffer.updating) {
                            try {
                                const nextData = this.videoQueue.shift();
                                this.sourceBuffer.appendBuffer(nextData);
                            } catch (e) {
                                console.error('Error processing video queue:', e);
                                this.videoQueue = [];
                                this.switchToCanvasRendering();
                            }
                        }
                        
                        // Auto-play if video is ready
                        if (this.videoScreen.paused && this.videoScreen.readyState >= 2) {
                            console.log('Starting video playback');
                            this.videoScreen.play().catch(e => {
                                console.warn('Auto-play failed:', e);
                                this.handleAutoplayFailed();
                            });
                        }
                    });
                    
                    this.sourceBuffer.addEventListener('error', (e) => {
                        console.error('SourceBuffer error:', e);
                        console.error('SourceBuffer error details:', {
                            readyState: this.mediaSource?.readyState,
                            updating: this.sourceBuffer?.updating,
                            buffered: this.sourceBuffer?.buffered.length || 0
                        });
                        this.needsKeyframe = true;
                        this.showNotification('Video decode error, switching to canvas mode');
                        this.switchToCanvasRendering();
                    });
                    
                    console.log('MediaSource and SourceBuffer initialized successfully');
                    
                    // Request initial keyframe
                    this.requestKeyframe();
                }
            } catch (e) {
                console.error('Error setting up MediaSource:', e);
                this.switchToCanvasRendering();
            }
        });
        
        this.mediaSource.addEventListener('error', (e) => {
            console.error('MediaSource error:', e);
            this.switchToCanvasRendering();
        });
    }

    switchToCanvasRendering() {
        console.log('Switching to canvas rendering mode');
        this.renderingMode = 'canvas';
        
        // Clean up MediaSource resources
        if (this.mediaSource) {
            try {
                if (this.mediaSource.readyState === 'open') {
                    this.mediaSource.endOfStream();
                }
            } catch (e) {
                console.warn('Error ending MediaSource:', e);
            }
            this.mediaSource = null;
        }
        
        this.sourceBuffer = null;
        this.videoQueue = [];
        
        // Reset keyframe requirements for canvas mode - be more lenient
        this.needsKeyframe = true;
        this.waitingForKeyframe = true;
        
        // Hide video element and show canvas
        if (this.videoScreen) {
            this.videoScreen.style.display = 'none';
            this.videoScreen.src = '';
        }
        if (this.canvasLayer) {
            this.canvasLayer.style.display = 'block';
        }
        
        // Set up canvas for rendering
        this.canvas = this.canvasLayer;
        this.ctx = this.canvas.getContext('2d');
        
        // Update status
        this.updateStatus('Canvas Mode', 'Using canvas rendering for video playback', false);
        
        // Request a keyframe immediately for canvas mode
        this.requestKeyframe();
        
        console.log('Canvas rendering mode activated, waiting for keyframe...');
    }

    switchToBlobFallback() {
        console.log('Switching to blob URL fallback (deprecated)');
        this.mediaSource = null;
        this.sourceBuffer = null;
        this.videoQueue = [];
        
        // This method is now deprecated - switch to canvas instead
        this.switchToCanvasRendering();
    }

    handleVideoFrame(data) {
        console.log('Received video frame:', data.codec, 'size:', data.data ? data.data.length : 0);
        
        if (!data.data) {
            console.error('No video data received');
            return;
        }
        
        try {
            // Handle different rendering modes
            if (this.renderingMode === 'canvas') {
                this.handleCanvasFrame(data);
                return;
            }
            
            const videoData = this.base64ToArrayBuffer(data.data);
            console.log('Decoded video data size:', videoData.byteLength);
            
            // Validate H.264 data format
            if (data.codec === 'h264' && !this.isValidVideoData(videoData)) {
                console.warn('Invalid video data format, switching to canvas mode');
                this.switchToCanvasRendering();
                return;
            }
            
            // Check if we have a valid MediaSource setup
            if (this.mediaSource && this.sourceBuffer && this.mediaSource.readyState === 'open') {
                console.log('Using MediaSource to play video frame');
                if (this.sourceBuffer.updating) {
                    // Queue the data if source buffer is busy
                    this.videoQueue.push(videoData);
                    // Limit queue size to prevent memory issues
                    if (this.videoQueue.length > 10) {
                        console.warn('Video queue getting large, dropping oldest frames');
                        this.videoQueue = this.videoQueue.slice(-5); // Keep only last 5 frames
                    }
                } else {
                    try {
                        this.sourceBuffer.appendBuffer(videoData);
                    } catch (e) {
                        console.error('Error appending video data:', e);
                        this.switchToCanvasRendering();
                    }
                }
            } else {
                console.log('MediaSource not ready, switching to canvas rendering');
                this.switchToCanvasRendering();
            }
            
            this.updateFrameStats();
            
        } catch (e) {
            console.error('Error handling video frame:', e);
            this.switchToCanvasRendering();
        }
    }

    handleCanvasFrame(data) {
        console.log('Rendering frame to canvas, is_keyframe:', data.is_keyframe);
        
        try {
            // Default to h264 if codec is not specified (WebRTC frames)
            const codec = data.codec || 'h264';
            
            if (codec === 'h264' || codec === 'webrtc') {
                const videoData = this.base64ToArrayBuffer(data.data);
                
                // For canvas mode, be more lenient with non-keyframes when accessed via network
                // We'll accept some non-keyframes to keep the video flowing
                if (this.needsKeyframe && !data.is_keyframe) {
                    // If we've been waiting for too long, be more accepting
                    const now = performance.now();
                    if (!this.lastKeyframeRequest || (now - this.lastKeyframeRequest) > 5000) {
                        console.log('Canvas mode: Been waiting too long, accepting non-keyframe');
                        this.needsKeyframe = false;
                    } else {
                        console.log('Canvas mode: Skipping non-keyframe while waiting for keyframe');
                        this.requestKeyframe();
                        return;
                    }
                }
                
                // Try to use WebCodecs API for H.264 decoding if available
                if (window.VideoDecoder && window.VideoFrame && !this.webCodecsFailed) {
                    this.handleWebCodecsDecoding(videoData, data.is_keyframe);
                } else {
                    // WebCodecs not available or failed - try alternative approaches
                    console.log('WebCodecs not available/failed, trying alternative canvas rendering');
                    this.handleCanvasFrameAlternative(data);
                }
                
                // Mark keyframe as processed
                if (data.is_keyframe) {
                    this.needsKeyframe = false;
                    this.waitingForKeyframe = false;
                    console.log('Keyframe processed in canvas mode');
                }
            } else {
                console.warn('Unsupported canvas frame format:', codec);
                this.renderPlaceholder('Unsupported Format: ' + codec);
            }
            
        } catch (e) {
            console.error('Error rendering canvas frame:', e);
            this.renderPlaceholder('Error: ' + e.message);
        }
    }

    handleCanvasFrameAlternative(data) {
        // When WebCodecs is not available, we can try a few alternative approaches
        console.log('Using alternative canvas rendering method');
        
        // Method 1: Try to create a video element and capture frames
        if (!this.canvasVideoElement) {
            this.canvasVideoElement = document.createElement('video');
            this.canvasVideoElement.muted = true;
            this.canvasVideoElement.playsInline = true;
            this.canvasVideoElement.autoplay = true;
            this.canvasVideoElement.style.display = 'none';
            document.body.appendChild(this.canvasVideoElement);
        }
        
        try {
            // Create a blob URL from the video data
            const videoData = this.base64ToArrayBuffer(data.data);
            
            // Try different MIME types for better compatibility
            const mimeTypes = [
                'video/mp4',
                'video/mp4; codecs="avc1.42E01E"',
                'video/webm',
                'video/webm; codecs="vp8"'
            ];
            
            let blob = null;
            for (const mimeType of mimeTypes) {
                try {
                    blob = new Blob([videoData], { type: mimeType });
                    break;
                } catch (e) {
                    console.warn('Failed to create blob with MIME type:', mimeType);
                }
            }
            
            if (!blob) {
                console.warn('Could not create blob with any MIME type, rendering placeholder');
                this.renderPlaceholder('Video Format Not Supported');
                return;
            }
            
            const url = URL.createObjectURL(blob);
            
            // Set up event handlers
            const cleanup = () => {
                URL.revokeObjectURL(url);
                this.canvasVideoElement.removeEventListener('loadeddata', onLoaded);
                this.canvasVideoElement.removeEventListener('error', onError);
            };
            
            const onLoaded = () => {
                try {
                    if (this.canvasLayer && this.ctx && this.canvasVideoElement.videoWidth > 0) {
                        // Set canvas size to match video
                        this.canvasLayer.width = this.canvasVideoElement.videoWidth;
                        this.canvasLayer.height = this.canvasVideoElement.videoHeight;
                        
                        // Draw the video frame to canvas
                        this.ctx.drawImage(this.canvasVideoElement, 0, 0);
                        
                        // Hide status display
                        this.hideStatusDisplay();
                        
                        if (data.is_keyframe) {
                            this.needsKeyframe = false;
                            this.waitingForKeyframe = false;
                            console.log('Keyframe processed via canvas alternative method');
                        }
                        
                        console.log('Successfully rendered frame via alternative method');
                    } else {
                        console.warn('Canvas or video element not ready for alternative rendering');
                    }
                } catch (e) {
                    console.error('Error drawing to canvas:', e);
                } finally {
                    cleanup();
                }
            };
            
            const onError = (e) => {
                console.warn('Canvas video element error:', e);
                cleanup();
                
                // If this is a keyframe and we're in network mode, try to show something
                if (data.is_keyframe) {
                    this.renderPlaceholder('Processing Video...\nCanvas Fallback Active');
                    this.needsKeyframe = false; // Don't get stuck waiting
                    this.waitingForKeyframe = false;
                } else {
                    this.renderPlaceholder('Video Processing Failed');
                }
            };
            
            this.canvasVideoElement.addEventListener('loadeddata', onLoaded, { once: true });
            this.canvasVideoElement.addEventListener('error', onError, { once: true });
            
            // Set the blob URL
            this.canvasVideoElement.src = url;
            
            // Force load attempt
            this.canvasVideoElement.load();
            
        } catch (e) {
            console.error('Alternative canvas rendering failed:', e);
            if (data.is_keyframe) {
                this.renderPlaceholder('Video Playback Active\n(Canvas Mode)');
                this.needsKeyframe = false; // Don't get stuck
                this.waitingForKeyframe = false;
            } else {
                this.renderPlaceholder('Alternative Rendering Failed');
            }
        }
    }

    handleWebCodecsDecoding(videoData, isKeyframe) {
        try {
            // Skip non-keyframes if we need a keyframe
            if (this.needsKeyframe && !isKeyframe) {
                console.log('Skipping non-keyframe for WebCodecs, requesting keyframe');
                this.requestKeyframe();
                return;
            }

            if (!this.videoDecoder) {
                console.log('Initializing WebCodecs VideoDecoder');
                
                this.videoDecoder = new VideoDecoder({
                    output: (frame) => {
                        console.log('WebCodecs decoded frame:', frame.displayWidth, 'x', frame.displayHeight);
                        
                        // Ensure canvas is properly sized
                        if (this.canvasLayer && this.ctx) {
                            this.canvasLayer.width = frame.displayWidth;
                            this.canvasLayer.height = frame.displayHeight;
                            
                            // Draw the frame to canvas
                            this.ctx.drawImage(frame, 0, 0);
                            
                            // Show the video and hide loading status
                            this.hideStatusDisplay();
                        }
                        
                        frame.close();
                    },
                    error: (e) => {
                        console.error('WebCodecs decode error:', e);
                        this.webCodecsFailed = true;
                        this.needsKeyframe = true;
                        
                        // Fall back to alternative canvas rendering
                        console.log('WebCodecs failed, falling back to alternative method');
                        this.handleCanvasFrameAlternative({ data: this.lastVideoData, is_keyframe: true });
                        
                        this.videoDecoder = null;
                    }
                });
                
                // Configure decoder for H.264
                this.videoDecoder.configure({
                    codec: 'avc1.42E01E', // H.264 Baseline Profile Level 3.0
                    optimizeForLatency: true
                });
            }
            
            if (this.videoDecoder.state === 'configured') {
                // Create EncodedVideoChunk
                const chunk = new EncodedVideoChunk({
                    type: isKeyframe ? 'key' : 'delta',
                    timestamp: Date.now() * 1000, // microseconds
                    data: videoData
                });
                
                this.videoDecoder.decode(chunk);
                
                if (isKeyframe) {
                    this.needsKeyframe = false;
                    console.log('Keyframe processed via WebCodecs');
                }
            }
            
        } catch (e) {
            console.error('WebCodecs decoding failed:', e);
            this.webCodecsFailed = true;
            this.needsKeyframe = true;
            
            // Fall back to alternative canvas rendering
            console.log('WebCodecs initialization failed, falling back to alternative method');
            this.handleCanvasFrameAlternative({ data: btoa(String.fromCharCode(...new Uint8Array(videoData))), is_keyframe: isKeyframe });
        }
    }

    renderPlaceholder(message) {
        if (this.canvasLayer && this.ctx) {
            this.ctx.fillStyle = '#1a1a1a';
            this.ctx.fillRect(0, 0, this.canvasLayer.width, this.canvasLayer.height);
            this.ctx.fillStyle = '#fff';
            this.ctx.font = '20px Arial';
            this.ctx.textAlign = 'center';
            this.ctx.fillText(message, this.canvasLayer.width / 2, this.canvasLayer.height / 2);
            this.ctx.font = '14px Arial';
            
            if (this.needsKeyframe) {
                this.ctx.fillText('Waiting for keyframe...', this.canvasLayer.width / 2, this.canvasLayer.height / 2 + 30);
            } else {
                this.ctx.fillText('Connecting...', this.canvasLayer.width / 2, this.canvasLayer.height / 2 + 30);
            }
        }
    }

    isValidVideoData(data) {
        // Basic validation for H.264 NAL units
        const view = new Uint8Array(data);
        
        // Check for NAL unit start codes (0x00 0x00 0x00 0x01 or 0x00 0x00 0x01)
        for (let i = 0; i < Math.min(view.length - 4, 100); i++) {
            if ((view[i] === 0x00 && view[i+1] === 0x00 && view[i+2] === 0x00 && view[i+3] === 0x01) ||
                (view[i] === 0x00 && view[i+1] === 0x00 && view[i+2] === 0x01)) {
                return true;
            }
        }
        
        return false;
    }

    requestKeyframe() {
        const now = performance.now();
        
        // Don't spam keyframe requests
        if (this.lastKeyframeRequest && (now - this.lastKeyframeRequest) < 1000) {
            return;
        }
        
        this.lastKeyframeRequest = now;
        console.log('Requesting keyframe from server');
        
        this.sendMessage({
            type: 'request_keyframe'
        });
    }

    handleAutoplayFailed() {
        // Show a play button overlay
        this.showPlayButton();
    }

    showPlayButton() {
        if (this.playButton) return; // Already showing
        
        this.playButton = document.createElement('button');
        this.playButton.textContent = '▶ Click to Play';
        this.playButton.className = 'play-button-overlay';
        this.playButton.style.cssText = `
            position: absolute;
            top: 50%;
            left: 50%;
            transform: translate(-50%, -50%);
            background: rgba(0,0,0,0.8);
            color: white;
            border: none;
            padding: 15px 25px;
            border-radius: 5px;
            font-size: 16px;
            cursor: pointer;
            z-index: 1000;
        `;
        
        this.playButton.onclick = () => {
            this.videoScreen.play().then(() => {
                this.playButton.remove();
                this.playButton = null;
            }).catch(console.error);
        };
        
        const screenElement = document.getElementById('screen');
        if (screenElement) {
            screenElement.appendChild(this.playButton);
        }
    }

    handleWebRTCFrame(data) {
        console.log('Received WebRTC frame:', {
            size: data.data ? data.data.length : 0,
            isKeyframe: data.is_keyframe,
            timestamp: data.timestamp,
            sequence: data.sequence_number
        });
        
        try {
            if (!data.data) {
                console.error('No WebRTC frame data received');
                return;
            }

            // WebRTC frames are always H.264 encoded - set codec if not present
            if (!data.codec) {
                data.codec = 'h264';
            }

            // Skip non-keyframes if we haven't received a keyframe yet
            if (this.needsKeyframe && !data.is_keyframe) {
                console.log('Skipping non-keyframe while waiting for keyframe');
                this.requestKeyframe();
                return;
            }

            if (data.is_keyframe) {
                this.needsKeyframe = false;
                console.log('Received keyframe, enabling playback');
            }

            // For WebRTC frames, we need to handle them differently than regular video frames
            // WebRTC frames are typically H.264 encoded and need to be displayed via video element or canvas
            if (this.renderingMode === 'canvas' || !this.videoScreen) {
                this.handleCanvasFrame(data);
            } else {
                // Try to display via video element with MediaSource API
                this.handleVideoFrameViaMediaSource(data);
            }
            
            this.updateFrameStats();
            
        } catch (e) {
            console.error('Error handling WebRTC frame:', e);
            // Fallback to canvas rendering
            this.switchToCanvasRendering();
        }
    }

    handleVideoFrameViaMediaSource(data) {
        try {
            // Skip non-keyframes if we need a keyframe
            if (this.needsKeyframe && !data.is_keyframe) {
                console.log('Skipping non-keyframe for MediaSource, requesting keyframe');
                this.requestKeyframe();
                return;
            }

            const videoData = this.base64ToArrayBuffer(data.data);
            
            // Check if we have a valid MediaSource setup
            if (!this.mediaSource || this.mediaSource.readyState !== 'open') {
                console.log('MediaSource not ready, switching to canvas mode');
                this.switchToCanvasRendering();
                return;
            }

            if (!this.sourceBuffer) {
                console.error('SourceBuffer not available, switching to canvas mode');
                this.switchToCanvasRendering();
                return;
            }

            // Validate that this looks like valid H.264 data
            if (!this.isValidVideoData(videoData)) {
                console.warn('Invalid H.264 data received, switching to canvas');
                this.switchToCanvasRendering();
                return;
            }

            // Check for additional MediaSource compatibility issues
            if (this.mediaSource.readyState === 'ended') {
                console.warn('MediaSource ended, switching to canvas mode');
                this.switchToCanvasRendering();
                return;
            }

            if (this.sourceBuffer.updating) {
                // Queue the data if source buffer is busy
                this.videoQueue.push(videoData);
                // Limit queue size to prevent memory issues
                if (this.videoQueue.length > 10) {
                    console.warn('Video queue getting large, dropping oldest frames');
                    this.videoQueue = this.videoQueue.slice(-5);
                }
            } else {
                try {
                    // Additional validation before appending
                    if (videoData.byteLength === 0) {
                        console.warn('Empty video data received, skipping');
                        return;
                    }

                    // For network connections, we might get corrupted data
                    // Let's be more defensive about appending
                    if (videoData.byteLength > 2 * 1024 * 1024) { // 2MB limit
                        console.warn('Video data too large:', videoData.byteLength, 'switching to canvas');
                        this.switchToCanvasRendering();
                        return;
                    }

                    this.sourceBuffer.appendBuffer(videoData);
                    if (data.is_keyframe) {
                        this.needsKeyframe = false;
                        console.log('Keyframe processed via MediaSource');
                    }
                } catch (e) {
                    console.error('Error appending WebRTC video data:', e);
                    console.error('Data size:', videoData.byteLength, 'isKeyframe:', data.is_keyframe);
                    console.error('SourceBuffer state:', {
                        updating: this.sourceBuffer.updating,
                        mode: this.sourceBuffer.mode,
                        buffered: this.sourceBuffer.buffered.length
                    });
                    console.error('MediaSource state:', this.mediaSource.readyState);
                    
                    // MediaSource failed, switch to canvas mode permanently for this session
                    this.needsKeyframe = true;
                    this.switchToCanvasRendering();
                }
            }
        } catch (e) {
            console.error('Error handling WebRTC video frame via MediaSource:', e);
            this.switchToCanvasRendering();
        }
    }

    handleStreamingStats(data) {
        console.log('Streaming stats:', data);
        
        // Update network stats display
        if (this.networkStats) {
            this.networkStats.innerHTML = `
                <div>Frames: ${data.frames_sent || 0}</div>
                <div>Bitrate: ${data.current_bitrate_kbps || 0} kbps</div>
                <div>Latency: ~${this.latency || 0}ms</div>
            `;
        }
    }

    // WebRTC quality switching
    switchQuality(quality) {
        if (this.websocket && this.websocket.readyState === WebSocket.OPEN) {
            this.websocket.send(JSON.stringify({
                type: 'quality_change',
                quality: quality
            }));
            
            this.showNotification(`Quality changed to ${quality}`, 2000);
            console.log(`Quality switched to: ${quality}`);
        }
    }

    // Auto quality adaptation based on network stats
    autoAdaptQuality() {
        if (!this.config.adaptiveQuality) return;
        
        const stats = this.networkStats;
        let recommendedQuality = 'medium';
        
        // High quality: Good bandwidth (>6 Mbps), low latency (<50ms), minimal packet loss (<1%)
        if (stats.bandwidth > 6000 && stats.latency < 50 && stats.packetLoss < 1.0) {
            recommendedQuality = 'high';
        }
        // Low quality: Poor conditions
        else if (stats.bandwidth < 2000 || stats.latency > 200 || stats.packetLoss > 5.0) {
            recommendedQuality = 'low';
        }
        
        if (recommendedQuality !== this.currentQuality) {
            this.currentQuality = recommendedQuality;
            this.switchQuality(recommendedQuality);
        }
    }

    // Start network monitoring and adaptive quality
    startNetworkMonitoring() {
        // Send network stats every 5 seconds
        this.networkMonitoringInterval = setInterval(() => {
            this.sendNetworkStats();
            if (this.adaptiveQuality) {
                this.autoAdaptQuality();
            }
        }, 5000);
    }

    // Stop network monitoring
    stopNetworkMonitoring() {
        if (this.networkMonitoringInterval) {
            clearInterval(this.networkMonitoringInterval);
            this.networkMonitoringInterval = null;
        }
    }

    // Update network stats display
    updateNetworkStats(stats) {
        this.networkStats = stats;
        
        // Update UI if stats are visible
        if (this.showStats) {
            const bandwidthDisplay = document.getElementById('bandwidth-display');
            const latencyDisplay = document.getElementById('latency-display');
            const packetLossDisplay = document.getElementById('packet-loss-display');
            
            if (bandwidthDisplay) {
                bandwidthDisplay.textContent = `${(stats.bandwidth / 1000).toFixed(1)} Mbps`;
            }
            if (latencyDisplay) {
                latencyDisplay.textContent = `${stats.latency}ms`;
            }
            if (packetLossDisplay) {
                packetLossDisplay.textContent = `${stats.packetLoss.toFixed(1)}%`;
            }
        }
    }

    normalizeCodec(codec) {
        // Normalize codec names - server may send different variations
        if (!codec) return 'h264';
        
        const lowerCodec = codec.toLowerCase();
        if (lowerCodec === 'webrtc' || lowerCodec === 'h264' || lowerCodec.includes('avc')) {
            return 'h264';
        }
        
        // Default to h264 since it's our only supported codec
        return 'h264';
    }

    getCodecConfigurations(codec) {
        // Since we only support H.264 WebRTC, return H.264 codec configurations
        const normalizedCodec = this.normalizeCodec(codec);
        console.log(`Getting codec configurations for: ${normalizedCodec}`);
        
        if (normalizedCodec === 'h264') {
            return [
                'video/mp4; codecs="avc1.42E01E"',  // H.264 Baseline Profile
                'video/mp4; codecs="avc1.4D401E"',  // H.264 Main Profile
                'video/mp4; codecs="avc1.64001E"',  // H.264 High Profile
                'video/mp4; codecs="avc1.42001E"',  // H.264 Baseline Profile (alternative)
                'video/mp4; codecs="avc1.4D4028"',  // H.264 Main Profile Level 4.0
                'video/mp4; codecs="avc1.640028"'   // H.264 High Profile Level 4.0
            ];
        }
        
        // Fallback - should not happen since we only support H.264
        console.warn(`Unsupported codec: ${codec}, falling back to H.264`);
        return [
            'video/mp4; codecs="avc1.42E01E"',
            'video/mp4; codecs="avc1.4D401E"',
            'video/mp4; codecs="avc1.64001E"'
        ];
    }

    updateStatus(title, message, showSpinner = false) {
        if (this.statusDisplay) {
            const titleElement = this.statusDisplay.querySelector('h2');
            const messageElement = this.statusDisplay.querySelector('p');
            const spinnerElement = this.statusDisplay.querySelector('.loading-spinner');
            
            if (titleElement) {
                titleElement.textContent = title;
            }
            if (messageElement) {
                messageElement.textContent = message;
            }
            if (spinnerElement) {
                spinnerElement.style.display = showSpinner ? 'block' : 'none';
            }
            
            // Show the status display
            this.statusDisplay.style.display = 'flex';
        }
    }

    hideStatusDisplay() {
        if (this.statusDisplay) {
            this.statusDisplay.style.display = 'none';
        }
   }

    // Utility methods
    base64ToArrayBuffer(base64) {
        // Remove data URL prefix if present
        const base64Data = base64.replace(/^data:.*,/, '');
        
        // Decode base64 string
        const binaryString = atob(base64Data);
        const bytes = new Uint8Array(binaryString.length);
        
        for (let i = 0; i < binaryString.length; i++) {
            bytes[i] = binaryString.charCodeAt(i);
        }
        
        return bytes.buffer;
    }

    // Check if we're accessing via network IP (not localhost)
    isNetworkAccess() {
        const hostname = window.location.hostname;
        return hostname !== 'localhost' && 
               hostname !== '127.0.0.1' && 
               hostname !== '::1' &&
               !hostname.startsWith('localhost');
    }
}

// Initialize the KVM client when the page loads
document.addEventListener('DOMContentLoaded', () => {
    // Get configuration from global variable set by the template
    const config = window.KVM_CONFIG || {
        stretch: false,
        mute: false,
        audio: false,
        remoteOnly: false,
        encryption: false,
        monitor: 0,
        codec: "h264"
    };

    // Initialize template components
    if (window.TemplateInitializer) {
        TemplateInitializer.initialize(config);
    } else {
        // Fallback initialization if template parts not loaded
        document.querySelectorAll('.toggle-switch').forEach(toggle => {
            const setting = toggle.dataset.setting;
            const checkbox = toggle.querySelector('input');
            
            if (checkbox) {
                toggle.addEventListener('click', () => {
                    checkbox.checked = !checkbox.checked;
                    toggle.classList.toggle('active', checkbox.checked);
                });
                
                toggle.classList.toggle('active', checkbox.checked);
            }
        });
    }

    // Initialize KVM client
    window.kvmClient = new KVMClient(config);
});
