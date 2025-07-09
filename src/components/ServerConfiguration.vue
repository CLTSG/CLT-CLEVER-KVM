<template>
  <div class="server-config" :class="{ disabled: disabled }">
    <div class="form-group">
      <label for="port">Port:</label>
      <input 
        id="port" 
        :value="serverPort" 
        @input="$emit('update:server-port', parseInt($event.target.value))"
        type="number" 
        min="1024" 
        max="65535"
        :disabled="disabled"
      />
    </div>
    
    <div class="form-group" v-if="monitors.length > 0">
      <label for="monitor">Monitor:</label>
      <select 
        id="monitor" 
        :value="settings.selectedMonitor"
        @change="$emit('update:selected-monitor', parseInt($event.target.value))"
        :disabled="disabled"
      >
        <option v-for="(monitor, index) in monitors" :key="index" :value="index">
          {{ monitor.name }} {{ monitor.is_primary ? '(Primary)' : '' }} - {{ monitor.width }}x{{ monitor.height }}
        </option>
      </select>
    </div>
    
    <PresetSelector 
      :settings="settings"
      :disabled="disabled"
      @apply-preset="$emit('apply-preset', $event)"
    />
    
    <AdvancedSettings 
      :settings="settings" 
      :disabled="disabled"
    />
  </div>
</template>

<script setup>
import PresetSelector from './PresetSelector.vue';
import AdvancedSettings from './AdvancedSettings.vue';

defineProps({
  serverPort: Number,
  settings: Object,
  monitors: Array,
  disabled: {
    type: Boolean,
    default: false
  }
});

defineEmits(['apply-preset', 'update:server-port', 'update:selected-monitor']);
</script>

<style scoped>
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

.server-config.disabled {
  opacity: 0.6;
  pointer-events: none;
}

input:disabled, select:disabled {
  background-color: #f5f5f5;
  cursor: not-allowed;
}
</style>
