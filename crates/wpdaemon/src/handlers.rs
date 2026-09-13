//! HTTP handlers module for `wpdaemon`.

/// User address endpoints module.
pub mod address;
/// Peer mesh SDP endpoints module.
pub mod peer;
/// SDP offer/answer endpoint module.
pub mod sdp;

pub use address::*;
pub use peer::*;
pub use sdp::*;
