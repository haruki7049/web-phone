//! WebRTC server daemon, STUN/TURN server, and peer mesh daemon for real-time audio.
//!
//! This crate provides the server-side implementation for the web-phone
//! audio transmission system. It supports:
//! - WebRTC SDP signaling & DataChannel audio streaming
//! - Built-in STUN and TURN NAT traversal server
//! - Daemon-to-daemon mesh interconnection for forwarding audio across server nodes

pub mod broadcast;
pub mod config;
pub mod connection;
pub mod peer;
pub mod stun;

// Re-export commonly used types
pub use broadcast::{AUDIO_BROADCAST, AudioMessage};
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
