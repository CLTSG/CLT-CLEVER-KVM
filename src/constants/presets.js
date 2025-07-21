export const presets = {
  default: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: false,
    useWebRTC: true,
    useVP8: true,
    hardwareAcceleration: false,
    audioBitrate: 128,
    videoBitrate: 4000,
    framerate: 30,
    qualityProfile: 'medium'
  },
  highQuality: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: false,
    useWebRTC: true,
    useVP8: true,
    hardwareAcceleration: true,
    audioBitrate: 192,
    videoBitrate: 8000,
    framerate: 60,
    qualityProfile: 'high'
  },
  lowBandwidth: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: false,
    useWebRTC: true,
    useVP8: true,
    hardwareAcceleration: false,
    audioBitrate: 64,
    videoBitrate: 1500,
    framerate: 24,
    qualityProfile: 'low'
  },
  secure: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: true,
    useWebRTC: true,
    useVP8: true,
    hardwareAcceleration: false,
    audioBitrate: 128,
    videoBitrate: 4000,
    framerate: 30,
    qualityProfile: 'medium'
  }
};
