//! Application-level functionality
//! 
//! This module contains application state, configuration,
//! and other app-wide functionality.

pub mod state;
pub mod commands;

pub use state::*;

/// Re-export APP_NAME from lib::constants for convenience
pub use crate::lib::APP_NAME;
