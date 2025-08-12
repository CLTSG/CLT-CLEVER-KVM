//! Codec implementations for video and audio encoding/decoding
//! 
//! This module contains various codec implementations optimized
//! for real-time streaming with minimal latency.

pub mod realtime_codec;
pub mod yuv420_encoder;

pub use realtime_codec::*;
pub use yuv420_encoder::*;
