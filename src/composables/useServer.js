import { ref, reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/tauri";

export function useServer() {
  const serverStatus = ref(false);
  const serverUrl = ref("");
  const serverPort = ref(9921);
  const loading = ref(false);
  const errorMessage = ref("");
  const monitors = ref([]);
  const loadingMonitors = ref(false);

  // Server settings
  const settings = reactive({
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: false,
    useWebRTC: true,
    useH264: true,
    useH265: false,
    useAV1: false,
    hardwareAcceleration: false,
    selectedMonitor: 0,
    audioBitrate: 128,
    videoBitrate: 4000,
    framerate: 30
  });

  const selectedCodec = computed(() => {
    if (settings.useH265) return 'h265';
    if (settings.useAV1) return 'av1';
    return 'h264';
  });

  async function loadMonitors() {
    loadingMonitors.value = true;
    try {
      monitors.value = await invoke("get_available_monitors");
      
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

  async function checkServerStatus() {
    try {
      serverStatus.value = await invoke("get_server_status");
      if (serverStatus.value) {
        serverUrl.value = await invoke("get_server_url");
      }
      await loadMonitors();
    } catch (error) {
      errorMessage.value = error;
    }
  }

  async function startServer() {
    loading.value = true;
    errorMessage.value = "";
    
    try {
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
          audioBitrate: settings.audioBitrate * 1000,
          videoBitrate: settings.videoBitrate * 1000,
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

  function buildUrlWithParams() {
    if (!serverUrl.value) return "";
    
    let url = serverUrl.value;
    const params = [];
    
    if (settings.useWebRTC) {
      params.push('audio=true');
    }
    
    if (settings.encryptionEnabled) {
      params.push('encryption=true');
    }
    
    params.push(`codec=${selectedCodec.value}`);
    
    if (settings.selectedMonitor > 0) {
      params.push(`monitor=${settings.selectedMonitor}`);
    }
    
    if (params.length > 0) {
      url += (url.includes('?') ? ';' : '?') + params.join(';');
    }
    
    return url;
  }

  function openUrl() {
    const url = buildUrlWithParams();
    if (url) {
      window.open(url, '_blank');
    }
  }

  function copyUrl() {
    const url = buildUrlWithParams();
    if (url) {
      navigator.clipboard.writeText(url);
    }
  }

  return {
    serverStatus,
    serverUrl,
    serverPort,
    loading,
    errorMessage,
    settings,
    monitors,
    loadingMonitors,
    selectedCodec,
    checkServerStatus,
    startServer,
    stopServer,
    openUrl,
    copyUrl,
    loadMonitors
  };
}
