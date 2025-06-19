<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";

const serverStatus = ref(false);
const serverUrl = ref("");
const serverPort = ref(9921);
const loading = ref(false);
const errorMessage = ref("");
const showAdvancedSettings = ref(false);
const showLogs = ref(false);
const deltaEncoding = ref(true);
const adaptiveQuality = ref(true);
const encryptionEnabled = ref(false);
const webrtcEnabled = ref(false);
const debugLog = ref("");
const errorLog = ref("");

// Check server status on mount
onMounted(async () => {
  try {
    serverStatus.value = await invoke("get_server_status");
    if (serverStatus.value) {
      serverUrl.value = await invoke("get_server_url");
    }
  } catch (error) {
    errorMessage.value = error;
  }
});

async function startServer() {
  loading.value = true;
  errorMessage.value = "";
  
  try {
    serverUrl.value = await invoke("start_server", { 
      port: serverPort.value,
      options: {
        deltaEncoding: deltaEncoding.value,
        adaptiveQuality: adaptiveQuality.value,
        encryption: encryptionEnabled.value,
        webrtc: webrtcEnabled.value
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
    
    if (webrtcEnabled.value) {
      params.push('audio=true');
    }
    
    if (encryptionEnabled.value) {
      params.push('encryption=true');
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
    
    if (webrtcEnabled.value) {
      params.push('audio=true');
    }
    
    if (encryptionEnabled.value) {
      params.push('encryption=true');
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
    <p class="description">Remote desktop system for your local network</p>

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
        
        <div class="advanced-toggle">
          <button @click="toggleAdvancedSettings" class="text-button">
            {{ showAdvancedSettings ? '⬆️ Hide Advanced Settings' : '⬇️ Show Advanced Settings' }}
          </button>
        </div>
        
        <div v-if="showAdvancedSettings" class="advanced-settings">
          <div class="setting-group">
            <label>
              <input type="checkbox" v-model="deltaEncoding" />
              Delta Encoding (only send changed screen parts)
            </label>
          </div>
          
          <div class="setting-group">
            <label>
              <input type="checkbox" v-model="adaptiveQuality" />
              Adaptive Quality (adjust based on network conditions)
            </label>
          </div>
          
          <div class="setting-group">
            <label>
              <input type="checkbox" v-model="encryptionEnabled" />
              Enable Encryption (secure connection)
            </label>
          </div>
          
          <div class="setting-group">
            <label>
              <input type="checkbox" v-model="webrtcEnabled" />
              Enable WebRTC Audio (experimental)
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
      </table>
      <p class="example">Example: <code>http://hostname:9921/kvm?stretch=true;mute=true</code></p>
      
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
            <strong>WebRTC Audio:</strong> Enables audio streaming (experimental feature).
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

input[type="number"] {
  padding: 0.5rem;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 1rem;
  width: 100px;
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
</style>
