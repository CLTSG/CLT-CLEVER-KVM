<script setup>
import { ref, onMounted, reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/tauri";

const serverStatus = ref(false);
const serverUrl = ref("");
const serverPort = ref(9921);
const loading = ref(false);
const errorMessage = ref("");
const showAdvancedSettings = ref(false);
const showLogs = ref(false);
const debugLog = ref("");
const errorLog = ref("");
const showPresets = ref(false);
const selectedPreset = ref("default");

// Server settings
const settings = reactive({
  deltaEncoding: true,
  adaptiveQuality: true,
  encryptionEnabled: false,
  useWebRTC: true,  // Default to true for better audio
  useH264: true,
  useH265: false,
  useAV1: false,
  hardwareAcceleration: false, // Default to software encoding
  selectedMonitor: 0,
  audioBitrate: 128,  // kbps
  videoBitrate: 4000, // kbps
  framerate: 30
});

// Available monitors
const monitors = ref([]);
const loadingMonitors = ref(false);

// Computed properties for codec selection
const selectedCodec = computed(() => {
  if (settings.useH265) return 'h265';
  if (settings.useAV1) return 'av1';
  return 'h264'; // Default
});

// Presets for different use cases
const presets = {
  default: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: false,
    useWebRTC: true,
    useH264: true,
    useH265: false,
    useAV1: false,
    hardwareAcceleration: false, // Use software by default
    audioBitrate: 128,
    videoBitrate: 4000,
    framerate: 30
  },
  highQuality: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: false,
    useWebRTC: true,
    useH264: false,
    useH265: true,  // Use H.265 for better quality
    useAV1: false,
    hardwareAcceleration: true, // Enable hardware for high quality
    audioBitrate: 192,
    videoBitrate: 8000, // Higher bitrate
    framerate: 60       // Higher framerate
  },
  lowBandwidth: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: false,
    useWebRTC: true,
    useH264: false,
    useH265: true,  // H.265 is more efficient
    useAV1: false,
    hardwareAcceleration: false, // Software encoding for compatibility
    audioBitrate: 64,   // Lower audio bitrate
    videoBitrate: 2000, // Lower video bitrate
    framerate: 24       // Lower framerate
  },
  secure: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: true, // Enable encryption
    useWebRTC: true,
    useH264: true,
    useH265: false,
    useAV1: false,
    hardwareAcceleration: false, // Software encoding for security/compatibility
    audioBitrate: 128,
    videoBitrate: 4000,
    framerate: 30
  }
};

// Check server status on mount
onMounted(async () => {
  try {
    serverStatus.value = await invoke("get_server_status");
    if (serverStatus.value) {
      serverUrl.value = await invoke("get_server_url");
    }
    
    // Load available monitors
    await loadMonitors();
  } catch (error) {
    errorMessage.value = error;
  }
});

async function loadMonitors() {
  loadingMonitors.value = true;
  try {
    monitors.value = await invoke("get_available_monitors");
    
    // If we have monitors, select the primary one by default
    const primaryIndex = monitors.value.findIndex(m => m.is_primary);
    if (primaryIndex >= 0) {
      settings.selectedMonitor = primaryIndex;
    }
  } catch (error) {
    console.error("Failed to load monitors:", error);
    monitors.value = [];
  } finally {
    loadingMonitors.value = false;
  }
}

function applyPreset(presetName) {
  const preset = presets[presetName];
  if (preset) {
    // Apply all preset settings
    Object.keys(preset).forEach(key => {
      if (key in settings) {
        settings[key] = preset[key];
      }
    });
    selectedPreset.value = presetName;
    showPresets.value = false;
  }
}

async function startServer() {
  loading.value = true;
  errorMessage.value = "";
  
  try {
    // Update codec selection based on computed property
    const codec = selectedCodec.value;
    
    serverUrl.value = await invoke("start_server", { 
      port: serverPort.value,
      options: {
        deltaEncoding: settings.deltaEncoding,
        adaptiveQuality: settings.adaptiveQuality,
        encryption: settings.encryptionEnabled,
        webrtc: settings.useWebRTC,
        h264: codec === 'h264',
        h265: codec === 'h265',
        av1: codec === 'av1',
        hardwareAcceleration: settings.hardwareAcceleration,
        monitor: settings.selectedMonitor,
        audioBitrate: settings.audioBitrate * 1000, // Convert to bps
        videoBitrate: settings.videoBitrate * 1000, // Convert to bps
        framerate: settings.framerate
      }
    });
    serverStatus.value = true;
  } catch (error) {
    errorMessage.value = error;
  } finally {
    loading.value = false;
  }
}

async function stopServer() {
  loading.value = true;
  errorMessage.value = "";
  
  try {
    await invoke("stop_server");
    serverStatus.value = false;
    serverUrl.value = "";
  } catch (error) {
    errorMessage.value = error;
  } finally {
    loading.value = false;
  }
}

function openUrl() {
  if (serverUrl.value) {
    let url = serverUrl.value;
    
    // Add advanced parameters if enabled
    const params = [];
    
    if (settings.useWebRTC) {
      params.push('audio=true');
    }
    
    if (settings.encryptionEnabled) {
      params.push('encryption=true');
    }
    
    // Use the selected codec
    params.push(`codec=${selectedCodec.value}`);
    
    if (settings.selectedMonitor > 0) {
      params.push(`monitor=${settings.selectedMonitor}`);
    }
    
    if (params.length > 0) {
      url += (url.includes('?') ? ';' : '?') + params.join(';');
    }
    
    window.open(url, '_blank');
  }
}

function copyUrl() {
  if (serverUrl.value) {
    let url = serverUrl.value;
    
    // Add advanced parameters if enabled
    const params = [];
    
    if (settings.useWebRTC) {
      params.push('audio=true');
    }
    
    if (settings.encryptionEnabled) {
      params.push('encryption=true');
    }
    
    // Use the selected codec
    params.push(`codec=${selectedCodec.value}`);
    
    if (settings.selectedMonitor > 0) {
      params.push(`monitor=${settings.selectedMonitor}`);
    }
    
    if (params.length > 0) {
      url += (url.includes('?') ? ';' : '?') + params.join(';');
    }
    
    navigator.clipboard.writeText(url);
  }
}

function toggleAdvancedSettings() {
  showAdvancedSettings.value = !showAdvancedSettings.value;
}

function toggleLogs() {
  showLogs.value = !showLogs.value;
  if (showLogs.value) {
    refreshLogs();
  }
}

async function refreshLogs() {
  try {
    const [debug, error] = await invoke("get_logs");
    debugLog.value = debug;
    errorLog.value = error;
  } catch (error) {
    errorMessage.value = `Failed to load logs: ${error}`;
  }
}
</script>

<template>
  <main class="container">
    <h1>Clever KVM</h1>
    <p class="description">High-performance remote desktop system for your local network</p>

    <div class="card">
      <h2>Server Status</h2>
      <div class="status-indicator" :class="{ active: serverStatus }"></div>
      <p>{{ serverStatus ? 'Running' : 'Stopped' }}</p>
      
      <div v-if="serverStatus" class="server-info">
        <p>Server URL:</p>
        <div class="url-display">
          <span class="url">{{ serverUrl }}</span>
          <button class="icon-button" @click="openUrl" title="Open in browser">🌐</button>
          <button class="icon-button" @click="copyUrl" title="Copy URL">📋</button>
        </div>
      </div>

      <div v-if="!serverStatus" class="server-config">
        <div class="form-group">
          <label for="port">Port:</label>
          <input id="port" v-model="serverPort" type="number" min="1024" max="65535" />
        </div>
        
        <div class="form-group" v-if="monitors.length > 0">
          <label for="monitor">Monitor:</label>
          <select id="monitor" v-model="settings.selectedMonitor">
            <option v-for="(monitor, index) in monitors" :key="index" :value="index">
              {{ monitor.name }} {{ monitor.is_primary ? '(Primary)' : '' }} - {{ monitor.width }}x{{ monitor.height }}
            </option>
          </select>
        </div>
        
        <div class="presets-section">
          <button @click="showPresets = !showPresets" class="text-button">
            {{ showPresets ? '⬆️ Hide Presets' : '⬇️ Show Presets' }}
          </button>
          
          <div v-if="showPresets" class="presets-container">
            <button 
              @click="applyPreset('default')" 
              class="preset-button"
              :class="{ 'active-preset': selectedPreset === 'default' }">
              Default
            </button>
            <button 
              @click="applyPreset('highQuality')" 
              class="preset-button"
              :class="{ 'active-preset': selectedPreset === 'highQuality' }">
              High Quality
            </button>
            <button 
              @click="applyPreset('lowBandwidth')" 
              class="preset-button"
              :class="{ 'active-preset': selectedPreset === 'lowBandwidth' }">
              Low Bandwidth
            </button>
            <button 
              @click="applyPreset('secure')" 
              class="preset-button"
              :class="{ 'active-preset': selectedPreset === 'secure' }">
              Secure
            </button>
          </div>
        </div>
        
        <div class="advanced-toggle">
          <button @click="toggleAdvancedSettings" class="text-button">
            {{ showAdvancedSettings ? '⬆️ Hide Advanced Settings' : '⬇️ Show Advanced Settings' }}
          </button>
        </div>
        
        <div v-if="showAdvancedSettings" class="advanced-settings">
          <div class="setting-group">
            <h4>Codec Selection</h4>
            <label>
              <input type="radio" v-model="settings.useH264" :value="true" 
                     @change="settings.useH265 = false; settings.useAV1 = false" />
              H.264 (Best compatibility)
            </label>
            <label>
              <input type="radio" v-model="settings.useH265" :value="true" 
                     @change="settings.useH264 = false; settings.useAV1 = false" />
              H.265/HEVC (Better quality, lower bandwidth)
            </label>
            <label>
              <input type="radio" v-model="settings.useAV1" :value="true" 
                     @change="settings.useH264 = false; settings.useH265 = false" />
              AV1 (Experimental, newest codec)
            </label>
          </div>
          
          <div class="setting-group">
            <h4>Performance</h4>
            <label>
              <input type="checkbox" v-model="settings.hardwareAcceleration" />
              Hardware Acceleration (uses GPU encoding if available)
            </label>
            <label>
              <input type="checkbox" v-model="settings.deltaEncoding" />
              Delta Encoding (only send changed screen parts)
            </label>
            <label>
              <input type="checkbox" v-model="settings.adaptiveQuality" />
              Adaptive Quality (adjust based on network conditions)
            </label>
          </div>
          
          <div class="setting-group">
            <h4>Bitrates & Quality</h4>
            <div class="slider-group">
              <label for="video-bitrate">Video Bitrate: {{ settings.videoBitrate }} kbps</label>
              <input type="range" id="video-bitrate" v-model="settings.videoBitrate"
                     min="1000" max="12000" step="500" />
            </div>
            
            <div class="slider-group">
              <label for="audio-bitrate">Audio Bitrate: {{ settings.audioBitrate }} kbps</label>
              <input type="range" id="audio-bitrate" v-model="settings.audioBitrate"
                     min="32" max="256" step="16" />
            </div>
            
            <div class="slider-group">
              <label for="framerate">Framerate: {{ settings.framerate }} FPS</label>
              <input type="range" id="framerate" v-model="settings.framerate"
                     min="15" max="60" step="5" />
            </div>
          </div>
          
          <div class="setting-group">
            <h4>Features</h4>
            <label>
              <input type="checkbox" v-model="settings.encryptionEnabled" />
              Enable Encryption (secure connection)
            </label>
            <label>
              <input type="checkbox" v-model="settings.useWebRTC" />
              Enable WebRTC Audio
            </label>
          </div>
        </div>
      </div>

      <div class="actions">
        <button 
          v-if="!serverStatus" 
          @click="startServer" 
          :disabled="loading"
          class="primary-button"
        >
          {{ loading ? 'Starting...' : 'Start Server' }}
        </button>
        <button 
          v-else 
          @click="stopServer" 
          :disabled="loading"
          class="danger-button"
        >
          {{ loading ? 'Stopping...' : 'Stop Server' }}
        </button>
      </div>

      <p v-if="errorMessage" class="error">{{ errorMessage }}</p>
    </div>

    <div class="card">
      <h2>Connection Options</h2>
      <p>Add these parameters to the URL:</p>
      <table class="options-table">
        <tr>
          <td><code>stretch=true</code></td>
          <td>Stretch screen to fit window</td>
        </tr>
        <tr>
          <td><code>mute=true</code></td>
          <td>Mute audio</td>
        </tr>
        <tr>
          <td><code>audio=true</code></td>
          <td>Enable audio streaming</td>
        </tr>
        <tr>
          <td><code>remoteOnly=true</code></td>
          <td>Only show remote screen (no toolbar)</td>
        </tr>
        <tr>
          <td><code>encryption=true</code></td>
          <td>Enable encrypted connection</td>
        </tr>
        <tr>
          <td><code>codec=h264|h265|av1</code></td>
          <td>Select video codec (h264 is default)</td>
        </tr>
        <tr>
          <td><code>monitor=1</code></td>
          <td>Select specific monitor to display</td>
        </tr>
      </table>
      <p class="example">Example: <code>http://hostname:9921/kvm?stretch=true;codec=h265;monitor=1</code></p>
      
      <div class="features-section">
        <h3>Advanced Features</h3>
        <ul>
          <li>
            <strong>Delta Encoding:</strong> Only sends parts of the screen that have changed, reducing bandwidth usage.
          </li>
          <li>
            <strong>Adaptive Quality:</strong> Automatically adjusts image quality based on network conditions.
          </li>
          <li>
            <strong>Encryption:</strong> Secures the connection between client and server.
          </li>
          <li>
            <strong>WebRTC Audio:</strong> Enables audio streaming with low latency.
          </li>
          <li>
            <strong>Multiple Codecs:</strong> 
            <ul>
              <li><strong>H.264:</strong> Widely compatible, good performance.</li>
              <li><strong>H.265/HEVC:</strong> Better quality at lower bitrates, less compatible.</li>
              <li><strong>AV1:</strong> Next-generation codec, best quality but limited hardware support.</li>
            </ul>
          </li>
          <li>
            <strong>Hardware Acceleration:</strong> Uses GPU for encoding when available, reducing CPU usage.
          </li>
          <li>
            <strong>Multi-monitor Support:</strong> Choose which monitor to share from systems with multiple displays.
          </li>
        </ul>
      </div>
    </div>
    
    <div class="card">
      <div class="log-toggle">
        <button @click="toggleLogs" class="text-button">
          {{ showLogs ? '⬆️ Hide Logs' : '⬇️ Show Logs' }}
        </button>
        <button v-if="showLogs" @click="refreshLogs" class="text-button refresh">
          🔄 Refresh
        </button>
      </div>
      
      <div v-if="showLogs" class="logs-container">
        <div class="log-section">
          <h3>Error Log</h3>
          <pre class="log-content">{{ errorLog || 'No errors logged' }}</pre>
        </div>
        
        <div class="log-section">
          <h3>Debug Log</h3>
          <pre class="log-content">{{ debugLog || 'No debug logs available' }}</pre>
        </div>
      </div>
    </div>
  </main>
</template>

<style scoped>
.container {
  max-width: 800px;
  margin: 0 auto;
  padding: 2rem;
}

h1 {
  font-size: 2.5rem;
  margin-bottom: 0.5rem;
  color: #2c3e50;
}

.description {
  font-size: 1.2rem;
  color: #7f8c8d;
  margin-bottom: 2rem;
}

.card {
  background-color: white;
  border-radius: 8px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  padding: 1.5rem;
  margin-bottom: 2rem;
}

h2 {
  margin-top: 0;
  color: #2c3e50;
  font-size: 1.5rem;
}

h3 {
  color: #2c3e50;
  font-size: 1.2rem;
  margin-top: 1.5rem;
  margin-bottom: 0.5rem;
}

h4 {
  color: #2c3e50;
  font-size: 1rem;
  margin-top: 1rem;
  margin-bottom: 0.5rem;
}

.status-indicator {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background-color: #e74c3c;
  display: inline-block;
  margin-right: 8px;
}

.status-indicator.active {
  background-color: #2ecc71;
}

.server-info {
  margin-top: 1rem;
  padding: 1rem;
  background-color: #f8f9fa;
  border-radius: 4px;
}

.url-display {
  display: flex;
  align-items: center;
  background-color: #ecf0f1;
  padding: 0.5rem;
  border-radius: 4px;
}

.url {
  flex: 1;
  font-family: monospace;
  word-break: break-all;
}

.icon-button {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 1.2rem;
  margin-left: 0.5rem;
  padding: 0.25rem;
}

.icon-button:hover {
  background-color: #dfe6e9;
  border-radius: 4px;
}

.server-config {
  margin-top: 1rem;
}

.form-group {
  margin-bottom: 1rem;
  display: flex;
  align-items: center;
}

.form-group label {
  margin-right: 1rem;
  min-width: 60px;
}

input[type="number"], select {
  padding: 0.5rem;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 1rem;
  min-width: 200px;
}

.advanced-toggle, .log-toggle {
  margin: 1rem 0;
  display: flex;
  align-items: center;
}

.text-button {
  background: none;
  border: none;
  color: #3498db;
  cursor: pointer;
  font-size: 0.9rem;
  padding: 0;
  text-decoration: underline;
}

.text-button.refresh {
  margin-left: 1rem;
}

.advanced-settings {
  background-color: #f8f9fa;
  border-radius: 4px;
  padding: 1rem;
  margin-bottom: 1rem;
}

.setting-group {
  margin-bottom: 0.75rem;
}

.setting-group label {
  display: flex;
  align-items: center;
  cursor: pointer;
}

.setting-group input[type="checkbox"] {
  margin-right: 0.5rem;
}

.actions {
  margin-top: 1.5rem;
  display: flex;
  gap: 1rem;
}

.primary-button {
  background-color: #3498db;
  color: white;
  border: none;
  padding: 0.75rem 1.5rem;
  border-radius: 4px;
  cursor: pointer;
  font-size: 1rem;
}

.primary-button:hover {
  background-color: #2980b9;
}

.danger-button {
  background-color: #e74c3c;
  color: white;
  border: none;
  padding: 0.75rem 1.5rem;
  border-radius: 4px;
  cursor: pointer;
  font-size: 1rem;
}

.danger-button:hover {
  background-color: #c0392b;
}

button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.error {
  color: #e74c3c;
  margin-top: 1rem;
}

.options-table {
  width: 100%;
  border-collapse: collapse;
  margin: 1rem 0;
}

.options-table td {
  padding: 0.5rem;
  border-bottom: 1px solid #eee;
}

.options-table td:first-child {
  font-family: monospace;
  white-space: nowrap;
}

code {
  background-color: #f8f9fa;
  padding: 0.25rem 0.5rem;
  border-radius: 4px;
  font-family: monospace;
}

.example {
  margin-top: 1rem;
  font-size: 0.9rem;
  color: #7f8c8d;
}

.features-section {
  margin-top: 1.5rem;
}

.features-section ul {
  padding-left: 1.5rem;
}

.features-section li {
  margin-bottom: 0.5rem;
}

.logs-container {
  margin-top: 1rem;
}

.log-section {
  margin-bottom: 1.5rem;
}

.log-content {
  background-color: #f8f9fa;
  padding: 1rem;
  border-radius: 4px;
  font-family: monospace;
  font-size: 0.8rem;
  white-space: pre-wrap;
  max-height: 300px;
  overflow-y: auto;
  border: 1px solid #eee;
}

.presets-section {
  margin: 1rem 0;
}

.presets-container {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 8px;
}

.preset-button {
  background-color: #f8f9fa;
  border: 1px solid #ddd;
  border-radius: 4px;
  padding: 8px 12px;
  cursor: pointer;
  transition: all 0.2s;
}

.preset-button:hover {
  background-color: #e9ecef;
}

.active-preset {
  background-color: #3498db;
  color: white;
  border-color: #2980b9;
}

.slider-group {
  margin-bottom: 12px;
}

.slider-group label {
  display: block;
  margin-bottom: 4px;
}

.slider-group input[type="range"] {
  width: 100%;
}
</style>
