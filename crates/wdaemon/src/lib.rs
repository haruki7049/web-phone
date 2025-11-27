//! WebTransport server daemon for real-time audio transmission.
//!
//! This crate provides the server-side implementation for the web-phone
//! audio transmission system. It accepts WebTransport connections and
//! broadcasts audio between all connected clients.
//!
//! # Architecture
//!
//! The server uses a broadcast pattern where audio received from any
//! client is sent to all connected clients. Each client receives a
//! unique ID and can filter their own audio based on their settings.
//!
//! # Modules
//!
//! - [`broadcast`] - Audio broadcast channel for distributing audio
//! - [`config`] - Server configuration types and defaults
//! - [`connection`] - Connection handling for WebTransport sessions
//!
//! # Protocol
//!
//! 1. Client connects via WebTransport
//! 2. Server sends 8-byte client ID
//! 3. Client sends: 4-byte length + audio data
//! 4. Server broadcasts: 8-byte sender ID + 4-byte length + audio data

pub mod broadcast;
pub mod config;
pub mod connection;

// Re-export commonly used types
pub use broadcast::{AUDIO_BROADCAST, AudioMessage};
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
