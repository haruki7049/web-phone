//! UserKeypair cryptographic identity keypair module.

use super::nostr::parse_nostr_key_to_bytes;
use super::user::UserAddress;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
enum KeypairKind {
    Ed25519(ed25519_dalek::SigningKey),
    Secp256k1(k256::schnorr::SigningKey),
}

impl fmt::Debug for KeypairKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeypairKind::Ed25519(_) => write!(f, "KeypairKind::Ed25519(...)"),
            KeypairKind::Secp256k1(_) => write!(f, "KeypairKind::Secp256k1(...)"),
        }
    }
}

/// Cryptographic Identity Keypair for wpclient / wpdaemon supporting Ed25519 (WPIP-01..03) and Secp256k1 / Nostr (WPIP-16).
#[derive(Debug, Clone)]
pub struct UserKeypair {
    kind: KeypairKind,
}

impl UserKeypair {
    /// Generate a new random Ed25519 keypair.
    pub fn generate() -> Self {
        let mut rng = rand::rngs::OsRng;
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
        Self {
            kind: KeypairKind::Ed25519(signing_key),
        }
    }

    /// Construct Ed25519 `UserKeypair` from raw 32-byte secret key.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(bytes);
        Self {
            kind: KeypairKind::Ed25519(signing_key),
        }
    }

    /// Construct Secp256k1 / Nostr `UserKeypair` from raw 32-byte secret key.
    pub fn from_secp256k1_bytes(bytes: &[u8; 32]) -> Result<Self, String> {
        let signing_key = k256::schnorr::SigningKey::from_bytes(bytes)
            .map_err(|e| format!("Invalid secp256k1 secret key: {}", e))?;
        Ok(Self {
            kind: KeypairKind::Secp256k1(signing_key),
        })
    }

    /// Construct Secp256k1 / Nostr `UserKeypair` from `nsec1...` Bech32 or 64-char Hex string.
    pub fn from_nostr_key(key_str: &str) -> Result<Self, String> {
        let bytes = parse_nostr_key_to_bytes(key_str)?;
        Self::from_secp256k1_bytes(&bytes)
    }

    /// Return raw 32-byte secret key.
    pub fn to_bytes(&self) -> [u8; 32] {
        match &self.kind {
            KeypairKind::Ed25519(sk) => sk.to_bytes(),
            KeypairKind::Secp256k1(sk) => {
                let bytes: [u8; 32] = sk.to_bytes().into();
                bytes
            }
        }
    }

    /// Get `UserAddress` representing this keypair's public key (64 hex string).
    pub fn public_key_address(&self) -> UserAddress {
        match &self.kind {
            KeypairKind::Ed25519(sk) => {
                let verifying_key = sk.verifying_key();
                UserAddress::from_bytes(verifying_key.to_bytes())
            }
            KeypairKind::Secp256k1(sk) => {
                let verifying_key = sk.verifying_key();
                let bytes = verifying_key.to_bytes();
                UserAddress::new(hex::encode(bytes))
            }
        }
    }

    /// Sign arbitrary byte message using secret key, returning hex signature string.
    pub fn sign(&self, message: &[u8]) -> String {
        match &self.kind {
            KeypairKind::Ed25519(sk) => {
                use ed25519_dalek::Signer;
                let sig = sk.sign(message);
                hex::encode(sig.to_bytes())
            }
            KeypairKind::Secp256k1(sk) => {
                use k256::schnorr::signature::Signer;
                let sig: k256::schnorr::Signature = sk.sign(message);
                hex::encode(sig.to_bytes())
            }
        }
    }

    /// Check if keypair is Secp256k1 / Nostr key.
    pub fn is_secp256k1(&self) -> bool {
        matches!(self.kind, KeypairKind::Secp256k1(_))
    }

    /// Cryptographically verify an Ed25519 or Secp256k1 signature hex string for a given `UserAddress` public key.
    pub fn verify(pubkey_address: &UserAddress, message: &[u8], signature_hex: &str) -> bool {
        use ed25519_dalek::Verifier;
        let pubkey_bytes = pubkey_address.to_bytes();
        if signature_hex.len() == 128 {
            let Ok(verifying_key) = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_bytes) else {
                return false;
            };
            let mut sig_buf = [0u8; 64];
            if hex::decode_to_slice(signature_hex, &mut sig_buf).is_ok()
                && verifying_key
                    .verify(message, &ed25519_dalek::Signature::from_bytes(&sig_buf))
                    .is_ok()
            {
                return true;
            }
        }

        if let Ok(pubkey_bytes) = hex::decode(&pubkey_address.id) {
            let Ok(verifying_key) = k256::schnorr::VerifyingKey::from_bytes(&pubkey_bytes) else {
                return false;
            };
            let Ok(sig_bytes) = hex::decode(signature_hex) else {
                return false;
            };
            let Ok(sig) = k256::schnorr::Signature::try_from(sig_bytes.as_slice()) else {
                return false;
            };
            use k256::schnorr::signature::Verifier;
            if verifying_key.verify(message, &sig).is_ok() {
                return true;
            }
        }

        false
    }
}

/// Build HTTP `Authorization` header value per WPIP-02 / WPIP-03 (`WP-Ed25519`) or WPIP-16 (`WP-Secp256k1`).
pub fn build_authorization_header(keypair: &UserKeypair, sdp_offer: &str) -> (u64, String) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let payload = format!("{}:{}", timestamp, sdp_offer);
    let sig_hex = keypair.sign(payload.as_bytes());
    let scheme = if keypair.is_secp256k1() {
        "WP-Secp256k1"
    } else {
        "WP-Ed25519"
    };
    let header = format!(
        "{} {}:{}:{}",
        scheme,
        keypair.public_key_address().id,
        timestamp,
        sig_hex
    );
    (timestamp, header)
}
