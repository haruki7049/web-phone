//! Authentication verification and Anti-Replay cache module.

use super::keypair::UserKeypair;
use super::user::UserAddress;
use k256::schnorr::{
    Signature as SchnorrSignature, VerifyingKey as SchnorrVerifyingKey, signature::Verifier,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Maximum allowable time drift (±300 seconds) for Authorization header timestamps (WPIP-02 / WPIP-16).
pub const MAX_TIMESTAMP_DRIFT_SECS: u64 = 300;

/// Authentication and authorization errors per WPIP-02 / WPIP-10 / WPIP-16.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum AuthError {
    #[error("Invalid authorization scheme: {0}")]
    InvalidScheme(String),

    #[error("Invalid authorization header format (expected PubKey:Timestamp:Sig)")]
    InvalidHeaderFormat,

    #[error("Invalid timestamp in authorization header")]
    InvalidTimestamp,

    #[error("Authorization timestamp drift too large ({diff}s > {max}s)")]
    TimestampDrift { diff: u64, max: u64 },

    #[error("Invalid Ed25519 cryptographic signature")]
    InvalidEd25519Signature,

    #[error("Invalid secp256k1 public key format")]
    InvalidSecp256k1Key,

    #[error("Invalid secp256k1 Schnorr cryptographic signature")]
    InvalidSecp256k1Signature,

    #[error("Invalid hex encoding: {0}")]
    InvalidHex(String),

    #[error("Replay attack detected: signature already used")]
    ReplayDetected,

    #[error("Invalid TURN username format (expected timestamp:user_address_hex)")]
    InvalidTurnUsernameFormat,

    #[error("TURN credential expired (expired at {expired_at}, current time {current_time})")]
    TurnCredentialExpired { expired_at: u64, current_time: u64 },

    #[error("Invalid TURN credential signature")]
    InvalidTurnSignature,

    #[error("Invalid Base64 encoding in TURN credential")]
    InvalidTurnBase64,

    #[error("Authorization header required")]
    MissingHeader,
}

/// Maximum allowed entries in the anti-replay signature cache.
pub const MAX_ANTI_REPLAY_CACHE_SIZE: usize = 10_000;

/// Anti-replay signature cache to prevent handshake replay attacks within the valid timestamp window.
#[derive(Debug, Clone, Default)]
pub struct AntiReplayCache {
    signatures: Arc<RwLock<HashMap<String, u64>>>,
}

impl AntiReplayCache {
    pub fn new() -> Self {
        Self {
            signatures: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a signature has already been used within the acceptable time window.
    /// If signature is unique, records it and returns `Ok(())`.
    /// Otherwise returns `Err(AuthError::ReplayDetected)`.
    pub fn check_and_insert(
        &self,
        signature: &str,
        timestamp: u64,
        now: u64,
    ) -> Result<(), AuthError> {
        let mut map = self.signatures.write().unwrap();

        // Retain entries within 300s window relative to current time `now`
        map.retain(|_, &mut ts| now.abs_diff(ts) <= MAX_TIMESTAMP_DRIFT_SECS);

        if map.contains_key(signature) {
            return Err(AuthError::ReplayDetected);
        }

        // Bound cache size to prevent memory exhaustion DoS attacks
        if map.len() >= MAX_ANTI_REPLAY_CACHE_SIZE {
            let mut entries: Vec<(String, u64)> = map.drain().collect();
            entries.sort_by_key(|(_, ts)| *ts);
            let keep_count = MAX_ANTI_REPLAY_CACHE_SIZE / 2;
            for (sig, ts) in entries.into_iter().skip(keep_count) {
                map.insert(sig, ts);
            }
        }

        map.insert(signature.to_string(), timestamp);
        Ok(())
    }

    /// Clear all stored signatures from cache.
    pub fn clear(&self) {
        let mut map = self.signatures.write().unwrap();
        map.clear();
    }
}

/// Global anti-replay cache shared across signaling handshake verifications.
pub static GLOBAL_ANTI_REPLAY_CACHE: LazyLock<AntiReplayCache> =
    LazyLock::new(AntiReplayCache::new);

/// Verify HTTP `Authorization` header value per WPIP-02 Section 2.2 with a specific Anti-Replay cache.
pub fn verify_authorization_header_with_cache(
    header_val: &str,
    sdp_offer: &str,
    cache: &AntiReplayCache,
) -> Result<UserAddress, AuthError> {
    let val = header_val
        .strip_prefix("WP-Ed25519 ")
        .ok_or_else(|| AuthError::InvalidScheme(header_val.to_string()))?;
    let parts: Vec<&str> = val.split(':').collect();
    if parts.len() != 3 {
        return Err(AuthError::InvalidHeaderFormat);
    }
    let pubkey_hex = parts[0];
    let timestamp: u64 = parts[1].parse().map_err(|_| AuthError::InvalidTimestamp)?;
    let sig_hex = parts[2];

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.abs_diff(timestamp);
    if diff > MAX_TIMESTAMP_DRIFT_SECS {
        return Err(AuthError::TimestampDrift {
            diff,
            max: MAX_TIMESTAMP_DRIFT_SECS,
        });
    }

    let addr = UserAddress::new(pubkey_hex);
    let payload = format!("{}:{}", timestamp, sdp_offer);
    if !UserKeypair::verify(&addr, payload.as_bytes(), sig_hex) {
        return Err(AuthError::InvalidEd25519Signature);
    }

    cache.check_and_insert(sig_hex, timestamp, now)?;

    Ok(addr)
}

/// Verify HTTP `Authorization` header value per WPIP-02 Section 2.2 using the global Anti-Replay cache.
pub fn verify_authorization_header(
    header_val: &str,
    sdp_offer: &str,
) -> Result<UserAddress, AuthError> {
    verify_authorization_header_with_cache(header_val, sdp_offer, &GLOBAL_ANTI_REPLAY_CACHE)
}

/// Verify HTTP `Authorization` header value per WPIP-16 (secp256k1 BIP-340 Schnorr signature) with a specific Anti-Replay cache.
pub fn verify_secp256k1_authorization_header_with_cache(
    header_val: &str,
    sdp_offer: &str,
    cache: &AntiReplayCache,
) -> Result<UserAddress, AuthError> {
    let val = header_val
        .strip_prefix("WP-Secp256k1 ")
        .ok_or_else(|| AuthError::InvalidScheme(header_val.to_string()))?;
    let parts: Vec<&str> = val.split(':').collect();
    if parts.len() != 3 {
        return Err(AuthError::InvalidHeaderFormat);
    }
    let pubkey_hex = parts[0];
    let timestamp: u64 = parts[1].parse().map_err(|_| AuthError::InvalidTimestamp)?;
    let sig_hex = parts[2];

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.abs_diff(timestamp);
    if diff > MAX_TIMESTAMP_DRIFT_SECS {
        return Err(AuthError::TimestampDrift {
            diff,
            max: MAX_TIMESTAMP_DRIFT_SECS,
        });
    }

    let pubkey_bytes = hex::decode(pubkey_hex).map_err(|e| AuthError::InvalidHex(e.to_string()))?;
    let verifying_key = SchnorrVerifyingKey::from_bytes(&pubkey_bytes)
        .map_err(|_| AuthError::InvalidSecp256k1Key)?;

    let sig_bytes = hex::decode(sig_hex).map_err(|e| AuthError::InvalidHex(e.to_string()))?;
    let signature = SchnorrSignature::try_from(sig_bytes.as_slice())
        .map_err(|_| AuthError::InvalidSecp256k1Signature)?;

    let payload = format!("{}:{}", timestamp, sdp_offer);
    if verifying_key
        .verify(payload.as_bytes(), &signature)
        .is_err()
    {
        return Err(AuthError::InvalidSecp256k1Signature);
    }

    cache.check_and_insert(sig_hex, timestamp, now)?;

    Ok(UserAddress::new(pubkey_hex))
}

/// Verify HTTP `Authorization` header value per WPIP-16 (secp256k1 BIP-340 Schnorr signature) using the global Anti-Replay cache.
pub fn verify_secp256k1_authorization_header(
    header_val: &str,
    sdp_offer: &str,
) -> Result<UserAddress, AuthError> {
    verify_secp256k1_authorization_header_with_cache(
        header_val,
        sdp_offer,
        &GLOBAL_ANTI_REPLAY_CACHE,
    )
}

/// Verify HTTP `Authorization` header value per WPIP-02 / WPIP-16 with a specific Anti-Replay cache, automatically detecting scheme.
pub fn verify_any_authorization_header_with_cache(
    header_val: &str,
    sdp_offer: &str,
    cache: &AntiReplayCache,
) -> Result<UserAddress, AuthError> {
    if header_val.starts_with("WP-Secp256k1 ") {
        verify_secp256k1_authorization_header_with_cache(header_val, sdp_offer, cache)
    } else {
        verify_authorization_header_with_cache(header_val, sdp_offer, cache)
    }
}

/// Verify HTTP `Authorization` header value per WPIP-02 / WPIP-16 using the global Anti-Replay cache, automatically detecting scheme.
pub fn verify_any_authorization_header(
    header_val: &str,
    sdp_offer: &str,
) -> Result<UserAddress, AuthError> {
    verify_any_authorization_header_with_cache(header_val, sdp_offer, &GLOBAL_ANTI_REPLAY_CACHE)
}
