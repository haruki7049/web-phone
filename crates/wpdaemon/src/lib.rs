//! WebRTC server daemon, SFU group call router, and peer mesh daemon for real-time audio.
//!
//! This crate provides the server-side implementation for the web-phone
//! audio transmission system strictly conforming to WPIP specifications.
//! It supports:
//! - WebRTC SDP signaling & DataChannel audio streaming
//! - Selective Forwarding Unit (SFU) audio routing with Top-K active speaker selection (WPIP-08)
//! - DataChannel heartbeat Ping/Pong keep-alive session monitoring (WPIP-09)
//! - Daemon-to-daemon mesh interconnection for forwarding audio across server nodes (WPIP-05)

pub mod broadcast;
pub mod config;
pub mod connection;
pub mod constants;
pub mod error;
pub mod handlers;
pub mod peer;
pub mod rate_limit;
pub mod registry;

// Re-export commonly used types
pub use broadcast::{AUDIO_BROADCAST, AudioMessage};
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
pub use constants::*;
pub use error::SignalingError;
pub use handlers::*;
pub use rate_limit::{
    DATACHANNEL_RATE_LIMITER, DataChannelRateLimiter, GLOBAL_RATE_LIMITER, RateLimiter,
    rate_limit_middleware, sanitize_ip,
};
pub use registry::{CLIENT_REGISTRY, ClientRegistry, matches_address};
