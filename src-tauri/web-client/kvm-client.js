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
        // Main elements - only use video element for modern codecs
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
        
        // Always show video element for H.264/H.265/AV1
        if (this.videoScreen) {
            this.videoScreen.classList.remove('hidden');
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
        
        // Mouse events - only use videoScreen since we removed JPEG support
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
        
        // Always use video screen for mouse events
        const activeScreen = this.videoScreen;
        if (!activeScreen) return;
        
        const rect = activeScreen.getBoundingClientRect();
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
            case 'mouseup':
                let button = 'left';
                if (e.button === 1) button = 'middle';
                if (e.button === 2) button = 'right';
                eventData.type = e.type;
                eventData.button = button;
                e.preventDefault();
                break;
            case 'mousemove':
                eventData.type = 'mousemove';
                break;
            case 'wheel':
                eventData.type = 'wheel';
                eventData.delta_y = e.deltaY;
                eventData.delta_x = e.deltaX;
                e.preventDefault();
                break;
        }
        
        this.sendInputEvent(eventData);
    }

    handleKeyEvent(e, type) {
        if (!this.connected) return;
        
        // Don't capture browser shortcuts
        if (e.ctrlKey && (e.key === 'r' || e.key === 'F5' || e.key === 'w')) return;
        
        const modifiers = [];
        if (e.ctrlKey) modifiers.push('Control');
        if (e.altKey) modifiers.push('Alt');
        if (e.shiftKey) modifiers.push('Shift');
        if (e.metaKey) modifiers.push('Meta');
        
        const eventData = {
            type,
            key: e.key,
            code: e.code,
            modifiers
        };
        
        if (type === 'keydown') {
            eventData.repeat = e.repeat;
        }
        
        this.sendInputEvent(eventData);
        
        // Prevent default for most keys when focused on remote screen
        if (document.activeElement === this.videoScreen || (this.videoScreen && this.videoScreen.contains(document.activeElement))) {
            e.preventDefault();
        }
    }

    handleTouchEvent(e) {
        // Basic touch event handling - can be expanded for gestures
        if (!this.connected) return;
        
        e.preventDefault();
        
        const touches = e.changedTouches;
        
        if (touches.length === 1) {
            const touch = touches[0];
            const activeScreen = this.videoScreen;
            if (!activeScreen) return;
            
            const rect = activeScreen.getBoundingClientRect();
            const scaleX = this.screenWidth / rect.width;
            const scaleY = this.screenHeight / rect.height;
            
            const x = Math.floor((touch.clientX - rect.left) * scaleX);
            const y = Math.floor((touch.clientY - rect.top) * scaleY);
            
            let eventData = {
                x,
                y,
                monitor_id: this.getActiveMonitorId()
            };
            
            switch(e.type) {
                case 'touchstart':
                    eventData.type = 'mousedown';
                    eventData.button = 'left';
                    break;
                case 'touchmove':
                    eventData.type = 'mousemove';
                    break;
                case 'touchend':
                case 'touchcancel':
                    eventData.type = 'mouseup';
                    eventData.button = 'left';
                    break;
            }
            
            this.sendInputEvent(eventData);
        }
    }

    sendInputEvent(event) {
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(event));
        }
    }

    getActiveMonitorId() {
        if (this.availableMonitors.length > 0 && this.currentMonitor < this.availableMonitors.length) {
            return this.availableMonitors[this.currentMonitor].id;
        }
        return null;
    }

    // Utility methods
    sendPing() {
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.lastPingTime = Date.now();
            this.ws.send(JSON.stringify({ type: 'ping' }));
        }
    }

    handlePingResponse() {
        const pingTime = Date.now() - this.lastPingTime;
        this.latency = pingTime;
        const latencyElements = document.querySelectorAll('#latency');
        latencyElements.forEach(el => el.textContent = pingTime);
        this.sendNetworkStats();
    }

    sendNetworkStats() {
        if (this.connected && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify({
                type: 'network_stats',
                latency: this.latency,
                bandwidth: this.estimateBandwidth(),
                packet_loss: this.estimatePacketLoss()
            }));
        }
    }

    estimateBandwidth() {
        if (this.latency < 50) return 10.0;
        if (this.latency < 100) return 5.0;
        if (this.latency < 200) return 2.0;
        return 1.0;
    }

    estimatePacketLoss() {
        if (this.latency > 200) return 5.0;
        if (this.latency > 100) return 1.0;
        return 0.0;
    }

    showNotification(message, duration = 3000) {
        if (!this.notificationArea) return;
        
        const notification = document.createElement('div');
        notification.className = 'notification';
        notification.textContent = message;
        this.notificationArea.appendChild(notification);
        
        setTimeout(() => {
            notification.classList.add('fadeout');
            setTimeout(() => {
                if (notification.parentNode) {
                    notification.remove();
                }
            }, 300);
        }, duration);
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
    }

    // Missing connect method
    connect() {
        this.updateStatus('Connecting', 'Establishing connection to server...', true);
        
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/ws?monitor=${this.currentMonitor}&codec=${this.currentCodec}${this.config.audio ? '&audio=true' : ''}`;
        
        console.log('Connecting to:', wsUrl);
        
        this.ws = new WebSocket(wsUrl);
        
        this.ws.onopen = () => {
            this.connected = true;
            this.updateStatus('Connected', 'Connection established successfully');
            setTimeout(() => {
                if (this.statusDisplay) {
                    this.statusDisplay.style.display = 'none';
                }
            }, 2000);
            
            console.log('WebSocket connection established');
            
            // Start sending ping messages to measure latency
            this.pingInterval = setInterval(() => this.sendPing(), 2000);
            
            this.showNotification('Connected to server');
        };
        
        this.ws.onclose = () => {
            this.connected = false;
            this.updateStatus('Disconnected', 'Connection lost. Attempting to reconnect...', true);
            if (this.statusDisplay) {
                this.statusDisplay.style.display = 'block';
            }
            
            console.log('WebSocket connection closed');
            
            // Clear ping interval
            if (this.pingInterval) {
                clearInterval(this.pingInterval);
            }
            
            this.showNotification('Disconnected from server. Reconnecting...');
            
            // Try to reconnect after a delay
            setTimeout(() => this.connect(), 3000);
        };
        
        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            this.updateStatus('Connection Error', 'Failed to connect to server');
            this.showNotification('Connection error occurred');
        };
        
        this.ws.onmessage = (event) => {
            try {
                const data = JSON.parse(event.data);
                this.handleMessage(data);
            } catch (e) {
                console.error('Error processing message:', e);
            }
        };
    }

    handleMessage(data) {
        console.log('Received message:', data.type);
        
        switch(data.type) {
            case 'video_frame':
                this.handleVideoFrame(data);
                break;
            case 'info':
                this.handleServerInfo(data);
                break;
            case 'ping':
                this.handlePingResponse();
                break;
            case 'quality':
                this.handleQualityUpdate(data);
                break;
            case 'monitors':
                this.handleMonitorList(data);
                break;
            case 'webrtc_offer':
                this.handleWebRTCOffer(data);
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
            this.osdTitle.textContent = `${data.hostname} - ${data.monitor} (${data.width}x${data.height})`;
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
        
        // Always use video element for modern codecs
        if (this.videoScreen) {
            this.videoScreen.classList.remove('hidden');
        }
        
        // Initialize video for codec streaming
        this.initializeVideoStreaming();
        
        // Initialize WebRTC for audio if enabled
        if (this.config.audio && data.audio) {
            this.setupWebRTC(data.encryption);
        }
        
        this.showNotification(`Connected to ${data.hostname} - ${data.monitor} (${data.width}x${data.height}) using ${data.codec}`);
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
        if (!window.MediaSource) {
            console.warn('MediaSource API not supported, using blob URL fallback');
            this.mediaSource = null;
            this.sourceBuffer = null;
            return;
        }
        
        const codecString = this.getCodecString(codec);
        const mimeType = `video/mp4; codecs="${codecString}"`;
        
        console.log('Trying to initialize MediaSource with MIME type:', mimeType);
        
        if (!MediaSource.isTypeSupported(mimeType)) {
            console.warn(`MediaSource does not support ${mimeType}, using blob URL fallback`);
            this.mediaSource = null;
            this.sourceBuffer = null;
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
        
        // Create object URL and set it to video element
        const objectURL = URL.createObjectURL(this.mediaSource);
        this.videoScreen.src = objectURL;
        
        // Set up MediaSource event handlers
        this.mediaSource.addEventListener('sourceopen', () => {
            console.log('MediaSource opened, adding SourceBuffer');
            try {
                if (this.mediaSource.readyState === 'open') {
                    this.sourceBuffer = this.mediaSource.addSourceBuffer(mimeType);
                    this.sourceBuffer.mode = 'sequence';
                    
                    // Set up SourceBuffer event handlers
                    this.sourceBuffer.addEventListener('updateend', () => {
                        // Process queued video data
                        if (this.videoQueue.length > 0 && !this.sourceBuffer.updating && this.sourceBuffer.buffered.length > 0) {
                            try {
                                // Remove old data to prevent buffer overflow
                                const buffered = this.sourceBuffer.buffered;
                                if (buffered.length > 0) {
                                    const currentTime = this.videoScreen.currentTime;
                                    const bufferStart = buffered.start(0);
                                    const bufferEnd = buffered.end(buffered.length - 1);
                                    
                                    // Remove data that's more than 30 seconds old
                                    if (currentTime - bufferStart > 30) {
                                        const removeEnd = Math.min(currentTime - 10, bufferEnd);
                                        if (removeEnd > bufferStart) {
                                            this.sourceBuffer.remove(bufferStart, removeEnd);
                                            return; // Wait for remove to complete
                                        }
                                    }
                                }
                                
                                // Append next queued data
                                const nextData = this.videoQueue.shift();
                                this.sourceBuffer.appendBuffer(nextData);
                            } catch (e) {
                                console.error('Error processing video queue:', e);
                                // Clear queue and switch to blob fallback
                                this.videoQueue = [];
                                this.switchToBlobFallback();
                            }
                        }
                    });
                    
                    this.sourceBuffer.addEventListener('error', (e) => {
                        console.error('SourceBuffer error:', e);
                        this.switchToBlobFallback();
                    });
                    
                    this.sourceBuffer.addEventListener('abort', (e) => {
                        console.warn('SourceBuffer abort:', e);
                    });
                    
                    console.log('MediaSource initialized successfully for', codec);
                } else {
                    console.error('MediaSource is not in open state:', this.mediaSource.readyState);
                }
            } catch (e) {
                console.error('Error setting up SourceBuffer:', e);
                this.switchToBlobFallback();
            }
        });
        
        this.mediaSource.addEventListener('sourceended', () => {
            console.log('MediaSource ended');
        });
        
        this.mediaSource.addEventListener('sourceclose', () => {
            console.log('MediaSource closed');
            this.sourceBuffer = null;
        });
        
        this.mediaSource.addEventListener('error', (e) => {
            console.error('MediaSource error:', e);
            this.switchToBlobFallback();
        });
    }

    switchToBlobFallback() {
        console.warn('Switching to blob URL fallback for video playback');
        this.mediaSource = null;
        this.sourceBuffer = null;
        this.videoQueue = [];
    }

    handleVideoFrame(data) {
        console.log('Received video frame:', data.codec, 'size:', data.data ? data.data.length : 0);
        
        if (!data.data) {
            console.error('No video data received');
            return;
        }
        
        try {
            const videoData = this.base64ToArrayBuffer(data.data);
            console.log('Decoded video data size:', videoData.byteLength);
            
            // Check if we have a valid MediaSource setup
            if (this.mediaSource && this.sourceBuffer && this.mediaSource.readyState === 'open') {
                if (this.sourceBuffer.updating) {
                    // Queue the data if source buffer is busy
                    this.videoQueue.push(videoData);
                    // Limit queue size to prevent memory issues
                    if (this.videoQueue.length > 50) {
                        console.warn('Video queue getting large, dropping oldest frames');
                        this.videoQueue.shift();
                    }
                } else {
                    try {
                        // Check if buffer is getting too full
                        const buffered = this.sourceBuffer.buffered;
                        if (buffered.length > 0) {
                            const bufferEnd = buffered.end(buffered.length - 1);
                            const currentTime = this.videoScreen.currentTime;
                            
                            // If buffer is more than 10 seconds ahead, skip this frame
                            if (bufferEnd - currentTime > 10) {
                                console.warn('Buffer too full, skipping frame');
                                return;
                            }
                        }
                        
                        this.sourceBuffer.appendBuffer(videoData);
                        
                        // Auto-play if video is paused (for initial frame)
                        if (this.videoScreen.paused && this.videoScreen.readyState >= 2) {
                            this.videoScreen.play().catch(e => {
                                console.warn('Auto-play failed:', e);
                            });
                        }
                        
                    } catch (e) {
                        console.error('Error appending video data:', e);
                        // Switch to blob fallback
                        this.switchToBlobFallback();
                        this.playVideoWithBlob(videoData, data.codec);
                    }
                }
            } else {
                // Use blob URL fallback
                console.log('Using blob URL fallback');
                this.playVideoWithBlob(videoData, data.codec);
            }
            
            this.updateFrameStats();
        } catch (e) {
            console.error('Error handling video frame:', e);
        }
    }

    playVideoWithBlob(videoData, codec) {
        // For blob fallback, we need to create a proper MP4 container
        // This is a simplified approach - in reality, you'd need to create proper MP4 headers
        
        const codecString = this.getCodecString(codec);
        const mimeType = `video/mp4; codecs="${codecString}"`;
        
        // Create blob with MP4 container
        const blob = new Blob([videoData], { type: mimeType });
        const url = URL.createObjectURL(blob);
        
        // Clean up previous URL
        if (this.videoScreen.src && this.videoScreen.src.startsWith('blob:')) {
            URL.revokeObjectURL(this.videoScreen.src);
        }
        
        // For better compatibility, we'll use a different approach
        // Create a new video element for each frame (less efficient but more reliable)
        const tempVideo = document.createElement('video');
        tempVideo.src = url;
        tempVideo.autoplay = true;
        tempVideo.muted = true;
        tempVideo.playsInline = true;
        
        tempVideo.onloadeddata = () => {
            // Copy the frame to canvas and then to main video
            if (this.canvasLayer) {
                const canvas = this.canvasLayer;
                const ctx = this.ctx;
                
                canvas.width = this.screenWidth;
                canvas.height = this.screenHeight;
                
                tempVideo.oncanplay = () => {
                    ctx.drawImage(tempVideo, 0, 0, canvas.width, canvas.height);
                    
                    // Convert canvas to blob and set as video source
                    canvas.toBlob((canvasBlob) => {
                        if (canvasBlob) {
                            const canvasUrl = URL.createObjectURL(canvasBlob);
                            
                            // Clean up previous video URL
                            if (this.videoScreen.src && this.videoScreen.src.startsWith('blob:')) {
                                URL.revokeObjectURL(this.videoScreen.src);
                            }
                            
                            this.videoScreen.src = canvasUrl;
                            
                            // Clean up temporary URLs
                            setTimeout(() => {
                                URL.revokeObjectURL(url);
                                URL.revokeObjectURL(canvasUrl);
                            }, 100);
                        }
                    }, 'image/jpeg', 0.9);
                };
            }
        };
        
        tempVideo.onerror = (e) => {
            console.error('Error with blob video:', e);
            URL.revokeObjectURL(url);
        };
    }

    getCodecString(codec) {
        switch (codec) {
            case 'h264':
                // Use more compatible H.264 profile
                return 'avc1.42E01E'; // Baseline profile, level 3.0
            case 'h265':
                // Use compatible H.265 profile
                return 'hev1.1.6.L93.B0'; // Main profile
            case 'av1':
                return 'av01.0.04M.08';
            default:
                return 'avc1.42E01E';
        }
    }

    base64ToArrayBuffer(base64) {
        const binaryString = atob(base64);
        const bytes = new Uint8Array(binaryString.length);
        for (let i = 0; i < binaryString.length; i++) {
            bytes[i] = binaryString.charCodeAt(i);
        }
        return bytes.buffer;
    }

    updateFrameStats() {
        this.frameCount++;
        const now = Date.now();
        
        if (now - this.lastFpsUpdate >= 1000) {
            const fps = Math.round(this.frameCount * 1000 / (now - this.lastFpsUpdate));
            
            // Update FPS display
            const fpsElements = document.querySelectorAll('#fps');
            fpsElements.forEach(el => el.textContent = fps);
            
            this.frameCount = 0;
            this.lastFpsUpdate = now;
        }
    }

    updateStatsVisibility() {
        if (this.showStats && this.connected) {
            this.networkStats.classList.add('visible');
        } else {
            this.networkStats.classList.remove('visible');
        }
    }

    toggleSettings() {
        if (this.settingsPanel.classList.contains('visible')) {
            this.hideSettings();
        } else {
            this.showSettings();
        }
    }

    showSettings() {
        this.settingsPanel.classList.add('visible');
        this.showOSD(); // Keep OSD visible while settings are open
    }

    hideSettings() {
        this.settingsPanel.classList.remove('visible');
    }

    updateStatus(title, message, showSpinner = false) {
        if (!this.statusDisplay) return;
        
        const titleEl = this.statusDisplay.querySelector('h2');
        const messageEl = this.statusDisplay.querySelector('p');
        const spinner = this.statusDisplay.querySelector('.loading-spinner');
        
        if (titleEl) titleEl.textContent = title;
        if (messageEl) messageEl.textContent = message;
        
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