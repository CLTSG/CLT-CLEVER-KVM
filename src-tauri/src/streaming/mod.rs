//! Streaming module - Video and audio streaming functionality
//! 
//! This module provides comprehensive streaming capabilities including
//! real-time codecs, enhanced performance implementations, and stream handlers.

pub mod codecs;
pub mod enhanced;
pub mod handlers;

// Re-export all public items for backward compatibility
pub use codecs::*;
pub use enhanced::*;
pub use handlers::*;
