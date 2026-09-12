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

    #[error("409 Conflict: Address prefix is ambiguous (matches multiple active clients)")]
    AddressAmbiguous,

    #[error("404 Not Found: {0}")]
    NotFound(String),

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
            SignalingError::AddressAmbiguous => StatusCode::CONFLICT,
            SignalingError::NotFound(_) => StatusCode::NOT_FOUND,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signaling_error_status_codes_and_response() {
        let err_max = SignalingError::MaxConnectionsReached(100);
        assert_eq!(err_max.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(err_max.to_string().contains("100"));

        let err_mesh = SignalingError::MaxMeshPeersReached(50);
        assert_eq!(err_mesh.status_code(), StatusCode::SERVICE_UNAVAILABLE);

        let err_unauth = SignalingError::Unauthorized(wpapi::AuthError::MissingHeader);
        assert_eq!(err_unauth.status_code(), StatusCode::UNAUTHORIZED);

        let err_sdp = SignalingError::InvalidSdpOffer("bad sdp".into());
        assert_eq!(err_sdp.status_code(), StatusCode::BAD_REQUEST);

        let err_ambig = SignalingError::AddressAmbiguous;
        assert_eq!(err_ambig.status_code(), StatusCode::CONFLICT);

        let err_notfound = SignalingError::NotFound("not found".into());
        assert_eq!(err_notfound.status_code(), StatusCode::NOT_FOUND);

        let err_internal = SignalingError::InternalError("internal".into());
        assert_eq!(
            err_internal.status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );

        let err_rate = SignalingError::RateLimitExceeded;
        assert_eq!(err_rate.status_code(), StatusCode::TOO_MANY_REQUESTS);

        let response = err_sdp.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
