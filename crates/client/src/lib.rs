//! WebTransport client for real-time audio transmission

pub mod audio;
pub mod call;
pub mod config;

// Re-export commonly used types
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
