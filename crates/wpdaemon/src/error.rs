//! Structured error types for wpdaemon signaling and HTTP endpoints.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;
use wpapi::AuthError;

/// Structured signaling error types for wpdaemon HTTP handlers.
#[derive(Debug, Error)]
pub enum SignalingError {
    #[error("503 Service Unavailable: Maximum concurrent connections reached ({0})")]
    MaxConnectionsReached(usize),

    #[error("503 Service Unavailable: Maximum mesh peer connections reached ({0})")]
    MaxMeshPeersReached(usize),

    #[error("401 Unauthorized: {0}")]
    Unauthorized(#[from] AuthError),

    #[error("400 Bad Request: Invalid SDP offer: {0}")]
    InvalidSdpOffer(String),

    #[error("500 Internal Server Error: {0}")]
    InternalError(String),

    #[error("429 Too Many Requests: Rate limit exceeded")]
    RateLimitExceeded,
}

impl SignalingError {
    /// Get corresponding HTTP status code.
    pub fn status_code(&self) -> StatusCode {
        match self {
            SignalingError::MaxConnectionsReached(_) | SignalingError::MaxMeshPeersReached(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            SignalingError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            SignalingError::InvalidSdpOffer(_) => StatusCode::BAD_REQUEST,
            SignalingError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SignalingError::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
        }
    }
}

impl IntoResponse for SignalingError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        (status, self.to_string()).into_response()
    }
}
