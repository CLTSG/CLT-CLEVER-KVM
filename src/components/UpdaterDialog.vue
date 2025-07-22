<template>
  <div v-if="showUpdateDialog" class="update-dialog-overlay">
    <div class="update-dialog">
      <h3>{{ updateTitle }}</h3>
      <p>{{ updateMessage }}</p>
      
      <div v-if="updateStatus === 'available'" class="update-actions">
        <button @click="installUpdate" class="btn btn-primary">
          Install Update
        </button>
        <button @click="closeDialog" class="btn btn-secondary">
          Later
        </button>
      </div>
      
      <div v-if="updateStatus === 'downloading'" class="update-progress">
        <div class="progress-bar">
          <div class="progress-fill" :style="{ width: downloadProgress + '%' }"></div>
        </div>
        <p>Downloading... {{ downloadProgress }}%</p>
      </div>
      
      <div v-if="updateStatus === 'ready'" class="update-actions">
        <button @click="restartApp" class="btn btn-primary">
          Restart & Install
        </button>
      </div>
      
      <div v-if="updateStatus === 'error'" class="update-actions">
        <p class="error-message">{{ errorMessage }}</p>
        <button @click="closeDialog" class="btn btn-secondary">
          Close
        </button>
      </div>
    </div>
  </div>
</template>

<script>
import { ref, onMounted, onUnmounted } from 'vue'
import { checkUpdate, installUpdate } from '@tauri-apps/api/updater'
import { relaunch } from '@tauri-apps/api/process'
import { ask } from '@tauri-apps/api/dialog'

export default {
  name: 'UpdaterDialog',
  setup() {
    const showUpdateDialog = ref(false)
    const updateStatus = ref('checking') // checking, available, downloading, ready, error, none
    const updateTitle = ref('')
    const updateMessage = ref('')
    const downloadProgress = ref(0)
    const errorMessage = ref('')
    let unlisten = null

    const checkForUpdates = async () => {
      try {
        console.log('Checking for updates...')
        const update = await checkUpdate()
        
        if (update.shouldUpdate) {
          console.log('Update available:', update.manifest?.version)
          updateStatus.value = 'available'
          updateTitle.value = 'Update Available'
          updateMessage.value = `A new version (${update.manifest?.version}) is available. Would you like to install it?`
          showUpdateDialog.value = true
        } else {
          console.log('App is up to date')
          updateStatus.value = 'none'
        }
      } catch (error) {
        console.error('Failed to check for updates:', error)
        if (error.message && !error.message.includes('Could not fetch update')) {
          updateStatus.value = 'error'
          updateTitle.value = 'Update Check Failed'
          errorMessage.value = error.message
          showUpdateDialog.value = true
        }
      }
    }

    const performUpdate = async () => {
      try {
        updateStatus.value = 'downloading'
        updateTitle.value = 'Downloading Update'
        updateMessage.value = 'Please wait while the update is downloaded...'
        
        // Listen for download progress
        if (unlisten) unlisten()
        unlisten = await installUpdate()
        
        updateStatus.value = 'ready'
        updateTitle.value = 'Update Ready'
        updateMessage.value = 'The update has been downloaded and is ready to install.'
        
      } catch (error) {
        console.error('Failed to install update:', error)
        updateStatus.value = 'error'
        updateTitle.value = 'Update Failed'
        errorMessage.value = error.message
      }
    }

    const restartApp = async () => {
      try {
        await relaunch()
      } catch (error) {
        console.error('Failed to restart app:', error)
        updateStatus.value = 'error'
        errorMessage.value = 'Failed to restart the application'
      }
    }

    const closeDialog = () => {
      showUpdateDialog.value = false
      updateStatus.value = 'none'
    }

    // Check for updates on component mount
    onMounted(() => {
      // Check for updates after a short delay to let the app fully load
      setTimeout(checkForUpdates, 2000)
    })

    onUnmounted(() => {
      if (unlisten) {
        unlisten()
      }
    })

    return {
      showUpdateDialog,
      updateStatus,
      updateTitle,
      updateMessage,
      downloadProgress,
      errorMessage,
      installUpdate: performUpdate,
      restartApp,
      closeDialog,
      checkForUpdates
    }
  }
}
</script>

<style scoped>
.update-dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background-color: rgba(0, 0, 0, 0.5);
  display: flex;
  justify-content: center;
  align-items: center;
  z-index: 1000;
}

.update-dialog {
  background: white;
  border-radius: 8px;
  padding: 24px;
  max-width: 400px;
  width: 90%;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
}

.update-dialog h3 {
  margin: 0 0 16px 0;
  color: #333;
  font-size: 1.25rem;
}

.update-dialog p {
  margin: 0 0 16px 0;
  color: #666;
  line-height: 1.5;
}

.update-actions {
  display: flex;
  gap: 12px;
  justify-content: flex-end;
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  transition: background-color 0.2s;
}

.btn-primary {
  background-color: #007bff;
  color: white;
}

.btn-primary:hover {
  background-color: #0056b3;
}

.btn-secondary {
  background-color: #6c757d;
  color: white;
}

.btn-secondary:hover {
  background-color: #545b62;
}

.update-progress {
  margin: 16px 0;
}

.progress-bar {
  width: 100%;
  height: 8px;
  background-color: #e9ecef;
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 8px;
}

.progress-fill {
  height: 100%;
  background-color: #007bff;
  transition: width 0.3s ease;
}

.error-message {
  color: #dc3545;
  font-size: 14px;
  margin-bottom: 16px;
}

/* Dark theme support */
@media (prefers-color-scheme: dark) {
  .update-dialog {
    background: #2d3748;
    color: white;
  }

  .update-dialog h3 {
    color: white;
  }

  .update-dialog p {
    color: #cbd5e0;
  }

  .progress-bar {
    background-color: #4a5568;
  }
}
</style>
