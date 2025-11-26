//! WebTransport server daemon for real-time audio transmission

pub mod broadcast;
pub mod config;
pub mod connection;

// Re-export commonly used types
pub use broadcast::AUDIO_BROADCAST;
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
