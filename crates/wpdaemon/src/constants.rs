//! Global constants and protocol parameters for `wpdaemon`.

use std::time::Duration;

/// Maximum payload size allowed for audio and control protocol packets (1 MB / 1,048,576 bytes).
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Default initial Time-To-Live (TTL) hop limit for peer mesh packet routing (WPIP-05 Section 3.1).
pub const DEFAULT_INITIAL_TTL: u8 = 8;

/// Handshake timeout duration: close PeerConnection if DataChannel is not opened within 15s.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

/// ICE candidate gathering timeout duration (3s).
pub const ICE_GATHER_TIMEOUT: Duration = Duration::from_secs(3);

/// Maximum allowable time drift (±300 seconds) for Authorization header timestamps (WPIP-02 / WPIP-16).
pub const MAX_TIMESTAMP_DRIFT_SECS: u64 = 300;

/// HTTP SDP signaling endpoint rate limiter: token refill rate (tokens/sec).
pub const HTTP_SDP_RATE_LIMIT_REFILL: f64 = 5.0;

/// HTTP SDP signaling endpoint rate limiter: bucket capacity (max burst).
pub const HTTP_SDP_RATE_LIMIT_BURST: f64 = 10.0;

/// STUN UDP endpoint rate limiter: token refill rate (tokens/sec).
pub const STUN_RATE_LIMIT_REFILL: f64 = 20.0;

/// STUN UDP endpoint rate limiter: bucket capacity (max burst).
pub const STUN_RATE_LIMIT_BURST: f64 = 50.0;

/// WebRTC DataChannel packet rate limiter: token refill rate (packets/sec).
pub const DATACHANNEL_RATE_LIMIT_REFILL: f64 = 100.0;

/// WebRTC DataChannel packet rate limiter: bucket capacity (max burst).
pub const DATACHANNEL_RATE_LIMIT_BURST: f64 = 200.0;
