//! WebTransport client for real-time audio transmission.
//!
//! This crate provides a client implementation for the web-phone audio
//! transmission system. It connects to a WebTransport server and enables
//! bidirectional audio streaming using the microphone and speakers.
//!
//! # Modules
//!
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

pub mod audio;
pub mod call;
pub mod config;

// Re-export commonly used types
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH, TlsVerifyMode};
