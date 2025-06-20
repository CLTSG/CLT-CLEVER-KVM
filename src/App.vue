<script setup>
import { onMounted, computed } from "vue";
import { useServer } from "./composables/useServer";
import { presets } from "./constants/presets";

import TabContainer from "./components/TabContainer.vue";
import ServerStatus from "./components/ServerStatus.vue";
import ServerConfiguration from "./components/ServerConfiguration.vue";
import ConnectionOptions from "./components/ConnectionOptions.vue";
import LogViewer from "./components/LogViewer.vue";

const {
  serverStatus,
  serverUrl,
  serverPort,
  loading,
  errorMessage,
  settings,
  monitors,
  loadingMonitors,
  checkServerStatus,
  startServer,
  stopServer,
  openUrl,
  copyUrl
} = useServer();

onMounted(async () => {
  await checkServerStatus();
});

function applyPreset(presetName) {
  const preset = presets[presetName];
  if (preset) {
    Object.keys(preset).forEach(key => {
      if (key in settings) {
        settings[key] = preset[key];
      }
    });
  }
}

function updateServerPort(value) {
  serverPort.value = value;
}

function updateSelectedMonitor(value) {
  settings.selectedMonitor = value;
}

// Define tabs based on server status
const tabs = computed(() => {
  const baseTabs = [
    { id: 'status', label: 'Server Status' },
    { id: 'options', label: 'Connection Options' },
    { id: 'logs', label: 'Logs' }
  ];

  // Insert configuration tab when server is stopped
  if (!serverStatus.value) {
    baseTabs.splice(1, 0, { id: 'config', label: 'Configuration' });
  }

  return baseTabs;
});
</script>

<template>
  <main class="container">
    <div class="header">
      <h1>Clever KVM</h1>
      <p class="description">High-performance remote desktop system for your local network</p>
    </div>

    <TabContainer :tabs="tabs" default-tab="status">
      <template #status>
        <ServerStatus 
          :server-status="serverStatus"
          :server-url="serverUrl"
          :loading="loading"
          :error-message="errorMessage"
          :start-server="startServer"
          :stop-server="stopServer"
          :open-url="openUrl"
          :copy-url="copyUrl"
        />
      </template>

      <template #config v-if="!serverStatus">
        <div class="config-content">
          <h2>Server Configuration</h2>
          <ServerConfiguration 
            :server-port="serverPort"
            :settings="settings"
            :monitors="monitors"
            @apply-preset="applyPreset"
            @update:server-port="updateServerPort"
            @update:selected-monitor="updateSelectedMonitor"
          />
        </div>
      </template>

      <template #options>
        <ConnectionOptions />
      </template>

      <template #logs>
        <LogViewer />
      </template>
    </TabContainer>
  </main>
</template>

<style scoped>
.container {
  max-width: 1000px;
  margin: 0 auto;
  padding: 1rem;
  min-height: 100vh;
}

.header {
  text-align: center;
  margin-bottom: 2rem;
}

h1 {
  font-size: 2.5rem;
  margin-bottom: 0.5rem;
  color: #2c3e50;
}

.description {
  font-size: 1.2rem;
  color: #7f8c8d;
  margin-bottom: 0;
}

.config-content h2 {
  margin-top: 0;
  color: #2c3e50;
  font-size: 1.5rem;
  margin-bottom: 1.5rem;
}

@media (max-width: 768px) {
  .container {
    padding: 0.5rem;
  }
  
  h1 {
    font-size: 2rem;
  }
  
  .description {
    font-size: 1rem;
  }
}
</style>