<template>
  <div class="advanced-toggle">
    <button @click="showAdvancedSettings = !showAdvancedSettings" class="text-button">
      {{ showAdvancedSettings ? '⬆️ Hide Advanced Settings' : '⬇️ Show Advanced Settings' }}
    </button>
  </div>
  
  <div v-if="showAdvancedSettings" class="advanced-settings" :class="{ disabled: disabled }">
    <div class="setting-group">
      <h4>Codec Selection</h4>
      <label>
        <input type="radio" v-model="settings.useVP8" :value="true" 
               :disabled="disabled"
               checked />
        WebRTC VP8 (Optimized for real-time streaming)
      </label>
    </div>
    
    <div class="setting-group">
      <h4>Performance</h4>
      <label>
        <input type="checkbox" v-model="settings.hardwareAcceleration" :disabled="disabled" />
        Hardware Acceleration (uses GPU encoding if available)
      </label>
      <label>
        <input type="checkbox" v-model="settings.deltaEncoding" :disabled="disabled" />
        Delta Encoding (only send changed screen parts)
      </label>
      <label>
        <input type="checkbox" v-model="settings.adaptiveQuality" :disabled="disabled" />
        Adaptive Quality (adjust based on network conditions)
      </label>
    </div>
    
    <div class="setting-group">
      <h4>Bitrates & Quality</h4>
      <div class="slider-group">
        <label for="video-bitrate">Video Bitrate: {{ settings.videoBitrate }} kbps</label>
        <input type="range" id="video-bitrate" v-model="settings.videoBitrate"
               min="1000" max="12000" step="500" :disabled="disabled" />
      </div>
      
      <div class="slider-group">
        <label for="audio-bitrate">Audio Bitrate: {{ settings.audioBitrate }} kbps</label>
        <input type="range" id="audio-bitrate" v-model="settings.audioBitrate"
               min="32" max="256" step="16" :disabled="disabled" />
      </div>
      
      <div class="slider-group">
        <label for="framerate">Framerate: {{ settings.framerate }} FPS</label>
        <input type="range" id="framerate" v-model="settings.framerate"
               min="15" max="60" step="5" :disabled="disabled" />
      </div>
    </div>
    
    <div class="setting-group">
      <h4>Features</h4>
      <label>
        <input type="checkbox" v-model="settings.encryptionEnabled" :disabled="disabled" />
        Enable Encryption (secure connection)
      </label>
      <label>
        <input type="checkbox" v-model="settings.useWebRTC" :disabled="disabled" />
        Enable WebRTC Audio
      </label>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue';

defineProps({
  settings: Object,
  disabled: {
    type: Boolean,
    default: false
  }
});

const showAdvancedSettings = ref(false);
</script>

<style scoped>
.advanced-toggle {
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

h4 {
  color: #2c3e50;
  font-size: 1rem;
  margin-top: 1rem;
  margin-bottom: 0.5rem;
}

.advanced-settings.disabled {
  opacity: 0.6;
  pointer-events: none;
}

input:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}
</style>
