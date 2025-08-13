<template>
  <div class="tab-container">
    <div class="tab-header">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        @click="activeTab = tab.id"
        :class="['tab-button', { active: activeTab === tab.id }]"
      >
        {{ tab.label }}
      </button>
    </div>
    
    <div class="tab-content">
      <slot :name="activeTab"></slot>
    </div>
  </div>
</template>

<script setup>
import { ref } from 'vue';

const props = defineProps({
  tabs: {
    type: Array,
    required: true
  },
  defaultTab: {
    type: String,
    default: null
  }
});

const activeTab = ref(props.defaultTab || props.tabs[0]?.id);
</script>

<style scoped>
.tab-container {
  background-color: white;
  border-radius: 8px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  overflow: hidden;
  margin-bottom: 2rem;
}

.tab-header {
  display: flex;
  background-color: #f8f9fa;
  border-bottom: 1px solid #dee2e6;
}

.tab-button {
  background: none;
  border: none;
  padding: 1rem 1.5rem;
  cursor: pointer;
  font-size: 1rem;
  color: #6c757d;
  border-bottom: 3px solid transparent;
  transition: all 0.2s ease;
  white-space: nowrap;
}

.tab-button:hover {
  background-color: #e9ecef;
  color: #495057;
}

.tab-button.active {
  color: #3498db;
  border-bottom-color: #3498db;
  background-color: white;
}

.tab-content {
  padding: 1.5rem;
  min-height: 400px;
}

@media (max-width: 768px) {
  .tab-header {
    flex-wrap: wrap;
  }
  
  .tab-button {
    flex: 1;
    min-width: 120px;
    padding: 0.75rem 1rem;
    font-size: 0.9rem;
  }
  
  .tab-content {
    padding: 1rem;
    min-height: 300px;
  }
}
</style>
