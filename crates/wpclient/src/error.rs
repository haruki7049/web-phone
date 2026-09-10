//! Error types for `wpclient`.

use thiserror::Error;

/// Error representation for client operations.
#[derive(Error, Debug)]
pub enum ClientError {
    /// Failed to load or save configuration.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Audio input/output device initialization or stream failure.
    #[error("Audio device error: {0}")]
    AudioDevice(String),

    /// WebRTC or daemon session error.
    #[error("Session error: {0}")]
    Session(String),

    /// Terminal user interface error.
    #[error("TUI error: {0}")]
    Tui(String),

    /// Network or API request error.
    #[error("Network error: {0}")]
    Network(String),
}
