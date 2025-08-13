<template>
  <div class="update-checker">
    <button 
      @click="checkForUpdates" 
      :disabled="isChecking"
      class="btn btn-update"
    >
      <span v-if="isChecking">Checking...</span>
      <span v-else>Check for Updates</span>
    </button>
    
    <div v-if="lastChecked" class="last-checked">
      Last checked: {{ formatDate(lastChecked) }}
    </div>
    
    <div v-if="updateStatus" class="update-status" :class="updateStatus.type">
      {{ updateStatus.message }}
    </div>
  </div>
</template>

<script>
import { ref } from 'vue'
import { checkUpdate } from '@tauri-apps/api/updater'

export default {
  name: 'UpdateChecker',
  emits: ['update-found'],
  setup(props, { emit }) {
    const isChecking = ref(false)
    const lastChecked = ref(null)
    const updateStatus = ref(null)

    const checkForUpdates = async () => {
      if (isChecking.value) return

      isChecking.value = true
      updateStatus.value = null

      try {
        console.log('Manually checking for updates...')
        const update = await checkUpdate()
        
        lastChecked.value = new Date()
        
        if (update.shouldUpdate) {
          updateStatus.value = {
            type: 'success',
            message: `Update available: v${update.manifest?.version}`
          }
          emit('update-found', update)
        } else {
          updateStatus.value = {
            type: 'info',
            message: 'You are running the latest version'
          }
        }
      } catch (error) {
        console.error('Failed to check for updates:', error)
        lastChecked.value = new Date()
        
        if (error.message && error.message.includes('Could not fetch update')) {
          updateStatus.value = {
            type: 'info',
            message: 'No updates available (offline or no releases)'
          }
        } else {
          updateStatus.value = {
            type: 'error',
            message: `Update check failed: ${error.message}`
          }
        }
      } finally {
        isChecking.value = false
      }
    }

    const formatDate = (date) => {
      return date.toLocaleString()
    }

    return {
      isChecking,
      lastChecked,
      updateStatus,
      checkForUpdates,
      formatDate
    }
  }
}
</script>

<style scoped>
.update-checker {
  margin: 16px 0;
}

.btn-update {
  background-color: #28a745;
  color: white;
  border: none;
  padding: 8px 16px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  transition: background-color 0.2s;
}

.btn-update:hover:not(:disabled) {
  background-color: #218838;
}

.btn-update:disabled {
  background-color: #6c757d;
  cursor: not-allowed;
}

.last-checked {
  margin-top: 8px;
  font-size: 12px;
  color: #666;
}

.update-status {
  margin-top: 8px;
  padding: 8px 12px;
  border-radius: 4px;
  font-size: 14px;
}

.update-status.success {
  background-color: #d4edda;
  color: #155724;
  border: 1px solid #c3e6cb;
}

.update-status.info {
  background-color: #d1ecf1;
  color: #0c5460;
  border: 1px solid #bee5eb;
}

.update-status.error {
  background-color: #f8d7da;
  color: #721c24;
  border: 1px solid #f5c6cb;
}

/* Dark theme support */
@media (prefers-color-scheme: dark) {
  .last-checked {
    color: #cbd5e0;
  }
  
  .update-status.success {
    background-color: #2d5a2d;
    color: #a3d4a3;
    border-color: #4a7c4a;
  }

  .update-status.info {
    background-color: #2d4a5a;
    color: #a3c4d4;
    border-color: #4a6c7c;
  }

  .update-status.error {
    background-color: #5a2d2d;
    color: #d4a3a3;
    border-color: #7c4a4a;
  }
}
</style>
