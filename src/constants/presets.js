export const presets = {
  default: {
    deltaEncoding: true,
    adaptiveQuality: true,
    encryptionEnabled: false,
    useWebRTC: true,
    useH264: true,
    useH265: false,
    useAV1: false,
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
    useH264: true,
    useH265: false,
    useAV1: false,
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
    useH264: true,
    useH265: false,
    useAV1: false,
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
    useH264: true,
    useH265: false,
    useAV1: false,
    hardwareAcceleration: false,
    audioBitrate: 128,
    videoBitrate: 4000,
    framerate: 30,
    qualityProfile: 'medium'
  }
};
