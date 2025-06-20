<template>
  <div class="connection-options">
    <h2>Connection Options</h2>
    <p>Add these parameters to the URL:</p>
    <table class="options-table">
      <tbody>
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
      </tbody>
    </table>
    <p class="example">Example: <code>http://hostname:9921/kvm?stretch=true;codec=h265;monitor=1</code></p>
    
    <div class="features-section">
      <h3>Advanced Features</h3>
      <ul>
        <li><strong>Delta Encoding:</strong> Only sends parts of the screen that have changed, reducing bandwidth usage.</li>
        <li><strong>Adaptive Quality:</strong> Automatically adjusts image quality based on network conditions.</li>
        <li><strong>Encryption:</strong> Secures the connection between client and server.</li>
        <li><strong>WebRTC Audio:</strong> Enables audio streaming with low latency.</li>
        <li><strong>Multiple Codecs:</strong> 
          <ul>
            <li><strong>H.264:</strong> Widely compatible, good performance.</li>
            <li><strong>H.265/HEVC:</strong> Better quality at lower bitrates, less compatible.</li>
            <li><strong>AV1:</strong> Next-generation codec, best quality but limited hardware support.</li>
          </ul>
        </li>
        <li><strong>Hardware Acceleration:</strong> Uses GPU for encoding when available, reducing CPU usage.</li>
        <li><strong>Multi-monitor Support:</strong> Choose which monitor to share from systems with multiple displays.</li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.connection-options {
  max-width: 800px;
  margin: 0 auto;
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

h2, h3 {
  color: #2c3e50;
}

h2 {
  margin-top: 0;
  font-size: 1.5rem;
}

h3 {
  font-size: 1.2rem;
  margin-top: 1.5rem;
  margin-bottom: 0.5rem;
}

@media (max-width: 768px) {
  .options-table {
    font-size: 0.9rem;
  }
  
  .options-table td {
    padding: 0.375rem;
  }
}
</style>
