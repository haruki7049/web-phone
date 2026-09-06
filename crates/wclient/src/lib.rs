//! WebTransport client for real-time audio transmission.
//!
//! This crate provides a client implementation for the web-phone audio
//! transmission system. It connects to a WebTransport server and enables
//! bidirectional audio streaming using the microphone and speakers.
//!
//! # Modules
//!
//! - [`address`] - IPv6 address information for wclient user recognition
//! - [`audio`] - Audio device enumeration and management
//! - [`call`] - Audio call functionality (connect, transmit, receive)
//! - [`config`] - Client configuration types and defaults
//!
//! # Example
//!
//! ```ignore
//! use wclient::{Configuration, call::start_call};
//!
//! let config = Configuration::default();
//! start_call(&config).await?;
//! ```

pub mod address;
pub mod audio;
pub mod call;
pub mod config;
pub mod resample;

// Re-export commonly used types
pub use address::UserAddress;
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
