//! UserAddress definition and conversion utilities.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global counter for tie-breaking SHA-256 generation in the same nanosecond.
static TIME_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Temporary user address / ID generated from UNIX timestamp SHA-256 hash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct UserAddress {
    /// SHA-256 hash string (hexadecimal representation).
    pub id: String,
}

impl UserAddress {
    /// Create a `UserAddress` from a hex ID string.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    /// Generate a temporary `UserAddress` using SHA-256 hash derived from current UNIX timestamp.
    pub fn generate_from_time() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let count = TIME_COUNTER.fetch_add(1, Ordering::Relaxed);

        let mut hasher = Sha256::new();
        hasher.update(format!("{}-{}", nanos, count).as_bytes());
        let result = hasher.finalize();
        let hex_id = hex::encode(result);

        Self { id: hex_id }
    }

    /// Derive a deterministic 32-byte `UserAddress` from a room ID string according to WPIP-08.
    ///
    /// If `room_id` is already a valid 64-character hex string, it is used directly.
    /// Otherwise, a SHA-256 digest of `room_id` is computed to guarantee a valid 32-byte room address.
    pub fn from_room_id(room_id: &str) -> Self {
        let trimmed = room_id.trim();
        if trimmed.len() == 64 && hex::decode(trimmed).is_ok() {
            Self::new(trimmed)
        } else {
            let digest = Sha256::digest(trimmed.as_bytes());
            Self::from_bytes(digest.into())
        }
    }

    /// Derive a cryptographic `UserAddress` for a room host and room ID string according to WPIP-08 §1:
    /// `SHA-256(host_pubkey_bytes || room_id_bytes)`.
    pub fn from_room_id_and_host(room_id: &str, host_pubkey: &UserAddress) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(host_pubkey.to_bytes());
        hasher.update(room_id.trim().as_bytes());
        let digest = hasher.finalize();
        Self::from_bytes(digest.into())
    }

    /// Convert 32-byte SHA-256 raw bytes to `UserAddress`.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            id: hex::encode(bytes),
        }
    }

    /// Convert `UserAddress` to 32-byte raw array representation.
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut raw_buf = [0u8; 32];
        if let Ok(decoded) = hex::decode(&self.id) {
            let len = decoded.len().min(32);
            raw_buf[..len].copy_from_slice(&decoded[..len]);
        }
        raw_buf
    }

    /// Return a short 12-character preview string of the SHA-256 ID.
    pub fn short_id(&self) -> &str {
        if self.id.len() >= 12 {
            &self.id[..12]
        } else {
            &self.id
        }
    }

    /// Check if this UserAddress matches another UserAddress by exact identity equality.
    pub fn matches_prefix(&self, other: &UserAddress) -> bool {
        self.id == other.id
    }
}

impl fmt::Display for UserAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl Default for UserAddress {
    fn default() -> Self {
        Self::generate_from_time()
    }
}

impl FromStr for UserAddress {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}
