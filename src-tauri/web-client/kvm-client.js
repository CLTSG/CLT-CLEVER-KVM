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

        this.initializeElements();
        this.setupEventListeners();
        this.connect();
    }

    initializeElements() {
        // Main elements - only use video element for H.264/H.265/AV1
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
        this.settingsPanel = document.querySelector('.settings-panel');
        
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

        if (this.codecDropdown) {
            this.codecDropdown.addEventListener('change', (e) => {
                const newCodec = e.target.value;
                if (newCodec !== this.currentCodec) {
                    this.switchCodec(newCodec);
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

    switchCodec(codec) {
        console.log('Switching to codec:', codec);
        this.currentCodec = codec;
        // Reconnect with new codec
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
            this.availableMonitors.forEach((monitor, index) => {
                const option = document.createElement('option');
                option.value = index;
                option.textContent = `${monitor.name} ${monitor.is_primary ? '(Primary)' : ''} - ${monitor.width}x${monitor.height}`;
                this.monitorDropdown.appendChild(option);
            });
            
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
        // Connect to KVM server on port 9921, not the frontend dev server port
        const kvmServerHost = window.location.hostname + ':9921';
        const wsUrl = `${protocol}//${kvmServerHost}/ws?monitor=${this.currentMonitor}&codec=${this.currentCodec}${this.config.audio ? '&audio=true' : ''}`;
        
        console.log('Connecting to:', wsUrl);
        
        this.ws = new WebSocket(wsUrl);
        
        this.ws.onopen = () => {
            this.connected = true;
            this.updateStatus('Connected', 'Connection established successfully');
            console.log('WebSocket connection established');
            
            // Start sending ping messages to measure latency
            this.pingInterval = setInterval(() => {
                this.sendPing();
            }, 5000);
        };
        
        this.ws.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data);
                this.handleMessage(data);
            } catch (e) {
                console.error('Error parsing WebSocket message:', e);
            }
        };
        
        this.ws.onclose = () => {
            this.connected = false;
            if (this.pingInterval) {
                clearInterval(this.pingInterval);
                this.pingInterval = null;
            }
            
            this.updateStatus('Disconnected', 'Connection closed');
            
            // Attempt to reconnect after a delay
            setTimeout(() => {
                if (!this.connected) {
                    console.log('Attempting to reconnect...');
                    this.connect();
                }
            }, 3000);
        };
        
        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            this.updateStatus('Error', 'Connection failed');
        };
    }

    handleMessage(data) {
        switch(data.type) {
            case 'server_info':
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
        
        this.currentCodec = data.codec || this.config.codec;
        if (this.codecDropdown) {
            this.codecDropdown.value = this.currentCodec;
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
        
        // Initialize MediaSource for H.264/H.265 if supported
        if (this.currentCodec === 'h264' || this.currentCodec === 'h265') {
            this.initializeMediaSource(this.currentCodec);
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
        this.mediaSource = null;
        this.sourceBuffer = null;
        this.videoQueue = [];
        
        // Hide video element and show canvas
        this.videoScreen.style.display = 'none';
        this.canvasLayer.style.display = 'block';
        
        // Set up canvas for rendering
        this.canvas = this.canvasLayer;
        this.ctx = this.canvas.getContext('2d');
        
        // Request server to send JPEG frames instead of H.264
        this.sendMessage({
            type: 'switch_codec',
            codec: 'jpeg'
        });
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
            if (this.renderingMode === 'canvas' || data.codec === 'jpeg') {
                this.handleCanvasFrame(data);
                return;
            }
            
            const videoData = this.base64ToArrayBuffer(data.data);
            console.log('Decoded video data size:', videoData.byteLength);
            
            // Validate H.264/H.265 data format
            if ((data.codec === 'h264' || data.codec === 'h265') && !this.isValidVideoData(videoData)) {
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
        console.log('Rendering frame to canvas');
        
        try {
            if (data.codec === 'jpeg' || data.format === 'jpeg') {
                // Handle JPEG frame
                const img = new Image();
                img.onload = () => {
                    if (this.canvas && this.ctx) {
                        // Resize canvas if needed
                        if (this.canvas.width !== img.width || this.canvas.height !== img.height) {
                            this.canvas.width = img.width;
                            this.canvas.height = img.height;
                        }
                        
                        // Clear and draw
                        this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
                        this.ctx.drawImage(img, 0, 0);
                    }
                    URL.revokeObjectURL(img.src);
                };
                
                img.onerror = (e) => {
                    console.error('Error loading JPEG image:', e);
                };
                
                // Create blob URL for JPEG data
                const videoData = this.base64ToArrayBuffer(data.data);
                const blob = new Blob([videoData], { type: 'image/jpeg' });
                img.src = URL.createObjectURL(blob);
                
            } else {
                console.warn('Unsupported canvas frame format:', data.codec);
            }
            
        } catch (e) {
            console.error('Error rendering canvas frame:', e);
        }
    }

    isValidVideoData(data) {
        // Basic validation for H.264/H.265 NAL units
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
        
        this.screenElement.appendChild(this.playButton);
    }

    handleWebRTCFrame(data) {
        console.log('Received WebRTC frame');
        
        try {
            // This would handle WebRTC frame data
            // For now, just update frame statistics
            this.updateFrameStats();
            
        } catch (e) {
            console.error('Error handling WebRTC frame:', e);
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

    // Utility methods
    getCodecConfigurations(codec) {
        switch(codec) {
            case 'h264':
                return [
                    'video/mp4; codecs="avc1.42E01E"',  // H.264 Baseline Profile Level 3.0 (most compatible)
                    'video/mp4; codecs="avc1.42001E"',  // H.264 Baseline Profile Level 3.0 alternative
                    'video/mp4; codecs="avc1.4D401F"',  // H.264 Main Profile Level 3.1
                    'video/mp4; codecs="avc1.640028"',  // H.264 High Profile Level 4.0
                    'video/mp4; codecs="avc1"',         // Generic H.264
                    'video/webm; codecs="vp8"'          // VP8 fallback
                ];
            case 'h265':
                return [
                    'video/mp4; codecs="hev1.1.6.L93.B0"',  // H.265 Main Profile Level 3.1
                    'video/mp4; codecs="hvc1.1.6.L93.B0"',  // H.265 Main Profile Level 3.1 alternative
                    'video/mp4; codecs="hev1"',             // Generic H.265
                    'video/mp4; codecs="avc1.42E01E"',      // H.264 fallback
                    'video/webm; codecs="vp9"'              // VP9 fallback
                ];
            case 'av1':
                return [
                    'video/mp4; codecs="av01.0.01M.08"',    // AV1 Main Profile Level 3.0
                    'video/webm; codecs="av01.0.01M.08"',   // AV1 in WebM
                    'video/mp4; codecs="avc1.42E01E"',      // H.264 fallback
                    'video/webm; codecs="vp9"'              // VP9 fallback
                ];
            default:
                return [
                    'video/mp4; codecs="avc1.42E01E"',      // Default to H.264
                    'video/webm; codecs="vp8"'              // VP8 fallback
                ];
        }
    }

    getCodecString(codec) {
        switch(codec) {
            case 'h264':
                return 'avc1.42E01E'; // H.264 Baseline Profile Level 3.0 (most compatible)
            case 'h265':
                return 'hev1.1.6.L93.B0'; // H.265 Main Profile Level 3.1
            case 'av1':
                return 'av01.0.01M.08'; // AV1 Main Profile Level 3.0
            default:
                return 'avc1.42E01E'; // Default to H.264
        }
    }

    base64ToArrayBuffer(base64) {
        const binaryString = atob(base64);
        const len = binaryString.length;
        const bytes = new Uint8Array(len);
        for (let i = 0; i < len; i++) {
            bytes[i] = binaryString.charCodeAt(i);
        }
        return bytes.buffer;
    }

    updateStatus(title, message, showSpinner = false) {
        if (!this.statusDisplay) return;
        
        const titleEl = this.statusDisplay.querySelector('h2');
        const messageEl = this.statusDisplay.querySelector('p');
        const spinner = this.statusDisplay.querySelector('.loading-spinner');
        
        if (titleEl) titleEl.textContent = title;
        if (messageEl) messageEl.textContent = message;
        
        // Show status display
        this.statusDisplay.style.display = 'block';
        
        if (showSpinner) {
            if (!spinner) {
                const spinnerEl = document.createElement('div');
                spinnerEl.className = 'loading-spinner';
                this.statusDisplay.appendChild(spinnerEl);
            }
        } else {
            if (spinner) {
                spinner.remove();
            }
        }
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
