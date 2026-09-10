//! Top-level domain-specific error types for `wpapi`.

use crate::address::AuthError;
use crate::keystore::KeyStoreError;
use crate::protocol::ProtocolError;
use crate::sframe::SFrameError;
use thiserror::Error;

/// Unified domain-specific error enum for all `wpapi` operations.
#[derive(Debug, Error)]
pub enum WpapiError {
    /// Authentication or signature verification error.
    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),

    /// Keystore encryption/decryption or storage error.
    #[error("Keystore error: {0}")]
    KeyStore(#[from] KeyStoreError),

    /// Protocol packet encoding/decoding error.
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    /// SFrame zero-knowledge encryption/decryption error.
    #[error("SFrame E2EE error: {0}")]
    SFrame(#[from] SFrameError),

    /// Audio engine or capture/playback device error.
    #[error("Audio engine error: {0}")]
    Audio(String),

    /// Session or connection management error.
    #[error("Session error: {0}")]
    Session(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wpapi_error_conversions() {
        let auth_err: WpapiError = AuthError::InvalidEd25519Signature.into();
        assert!(matches!(auth_err, WpapiError::Auth(_)));

        let keystore_err: WpapiError = KeyStoreError::InvalidPassphrase.into();
        assert!(matches!(keystore_err, WpapiError::KeyStore(_)));

        let protocol_err: WpapiError = ProtocolError::EmptyPacket.into();
        assert!(matches!(protocol_err, WpapiError::Protocol(_)));
    }
}
