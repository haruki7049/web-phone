//! Ephemeral HMAC-SHA1 TURN token authentication module per WPIP-10.

use crate::address::UserAddress;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;

/// Ephemeral TURN Server credential payload (WPIP-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCredential {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
    pub expiration_timestamp: u64,
}

/// Maximum allowed TTL for ephemeral TURN credentials (24 hours).
pub const MAX_TURN_TTL_SECS: u64 = 86400;

fn derive_opaque_user_id(user_address: &UserAddress, server_secret: &[u8]) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(server_secret)
        .unwrap_or_else(|_| HmacSha256::new_from_slice(b"turn_secret_fallback").unwrap());
    mac.update(user_address.id.as_bytes());
    let res = mac.finalize().into_bytes();
    let mut s = String::with_capacity(32);
    for b in &res[..16] {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
    }
    s
}

/// Generate a time-limited ephemeral TURN credential for a client UserAddress.
pub fn generate_ephemeral_turn_credential(
    server_secret: &[u8],
    user_address: &UserAddress,
    turn_urls: Vec<String>,
    ttl_seconds: u64,
) -> TurnCredential {
    let effective_ttl = ttl_seconds.min(MAX_TURN_TTL_SECS);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let exp_timestamp = now + effective_ttl;

    let opaque_id = derive_opaque_user_id(user_address, server_secret);
    let username = format!("{}:{}", exp_timestamp, opaque_id);

    let mut mac =
        HmacSha1::new_from_slice(server_secret).expect("HMAC initialization failed for TURN key");
    mac.update(username.as_bytes());
    let mac_result = mac.finalize().into_bytes();
    let credential = BASE64_STANDARD.encode(mac_result);

    TurnCredential {
        urls: turn_urls,
        username,
        credential,
        expiration_timestamp: exp_timestamp,
    }
}

use crate::address::AuthError;

/// Verify an ephemeral TURN credential against the server secret and current time.
pub fn verify_ephemeral_turn_credential(
    server_secret: &[u8],
    username: &str,
    credential: &str,
) -> Result<UserAddress, AuthError> {
    let parts: Vec<&str> = username.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(AuthError::InvalidTurnUsernameFormat);
    }

    let exp_timestamp: u64 = parts[0].parse().map_err(|_| AuthError::InvalidTimestamp)?;
    let user_address_hex = parts[1];

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if now > exp_timestamp {
        return Err(AuthError::TurnCredentialExpired {
            expired_at: exp_timestamp,
            current_time: now,
        });
    }

    let mut mac =
        HmacSha1::new_from_slice(server_secret).expect("HMAC initialization failed for TURN key");
    mac.update(username.as_bytes());
    let expected_mac = mac.finalize().into_bytes();

    let provided_mac = BASE64_STANDARD
        .decode(credential)
        .map_err(|_| AuthError::InvalidTurnBase64)?;

    use subtle::ConstantTimeEq;

    let is_equal: bool = expected_mac
        .as_slice()
        .ct_eq(provided_mac.as_slice())
        .into();
    if !is_equal {
        return Err(AuthError::InvalidTurnSignature);
    }

    Ok(UserAddress::new(user_address_hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ephemeral_turn_credential_generation_and_verification() {
        let secret = b"super_secret_turn_key_12345";
        let user_addr =
            UserAddress::new("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        let urls = vec!["turn:node.web-phone.dev:3478?transport=udp".to_string()];

        let cred = generate_ephemeral_turn_credential(secret, &user_addr, urls.clone(), 600);
        assert_eq!(cred.urls, urls);
        // Ensure username does NOT leak raw user address ID
        assert!(!cred.username.contains(&user_addr.id));

        let verified_addr =
            verify_ephemeral_turn_credential(secret, &cred.username, &cred.credential)
                .expect("Verification should succeed");
        let expected_opaque = derive_opaque_user_id(&user_addr, secret);
        assert_eq!(verified_addr.id, expected_opaque);
    }

    #[test]
    fn test_ephemeral_turn_credential_expired() {
        let secret = b"super_secret_turn_key_12345";
        let user_addr = UserAddress::new("test_user_id");

        // Expired 100 seconds ago
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let exp_timestamp = now - 100;
        let opaque_id = derive_opaque_user_id(&user_addr, secret);
        let username = format!("{}:{}", exp_timestamp, opaque_id);

        let mut mac = HmacSha1::new_from_slice(secret).unwrap();
        mac.update(username.as_bytes());
        let credential = BASE64_STANDARD.encode(mac.finalize().into_bytes());

        let result = verify_ephemeral_turn_credential(secret, &username, &credential);
        assert!(matches!(
            result,
            Err(AuthError::TurnCredentialExpired { .. })
        ));
    }

    #[test]
    fn test_ephemeral_turn_credential_invalid_secret() {
        let secret1 = b"correct_secret";
        let secret2 = b"wrong_secret";
        let user_addr = UserAddress::new("test_user_id");

        let cred = generate_ephemeral_turn_credential(secret1, &user_addr, vec![], 600);
        let result = verify_ephemeral_turn_credential(secret2, &cred.username, &cred.credential);
        assert_eq!(result, Err(AuthError::InvalidTurnSignature));
    }

    #[test]
    fn test_turn_credential_max_ttl_and_privacy() {
        let secret = b"turn_secret_key";
        let user_addr =
            UserAddress::new("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

        // Request excessive TTL (e.g. 10 days = 864000s)
        let cred = generate_ephemeral_turn_credential(secret, &user_addr, vec![], 864000);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Expired timestamp must be capped at now + MAX_TURN_TTL_SECS
        assert!(cred.expiration_timestamp <= now + MAX_TURN_TTL_SECS + 5);

        // Username MUST NOT expose the raw user address string
        assert!(!cred.username.contains(&user_addr.id));
    }
}
