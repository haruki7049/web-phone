//! WebTransport server daemon for real-time audio transmission

pub mod broadcast;
pub mod config;
pub mod connection;

// Re-export commonly used types
pub use broadcast::{AudioMessage, AUDIO_BROADCAST};
pub use config::{Configuration, CONFIGURATION, DEFAULT_CONFIG_PATH};
