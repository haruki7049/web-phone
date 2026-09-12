//! Address / User ID module for identifying wpclient users.

pub mod auth;
pub mod keypair;
pub mod nostr;
pub mod user;

pub use auth::*;
pub use keypair::*;
pub use nostr::*;
pub use user::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_ed25519_keypair_sign_verify() {
        let keypair = UserKeypair::generate();
        let pubkey_addr = keypair.public_key_address();
        assert_eq!(pubkey_addr.id.len(), 64);

        let msg = b"1757275200:v=0\r\nt=0 0\r\n";
        let sig_hex = keypair.sign(msg);
        assert_eq!(sig_hex.len(), 128);

        assert!(UserKeypair::verify(&pubkey_addr, msg, &sig_hex));
        assert!(!UserKeypair::verify(&pubkey_addr, b"wrong_msg", &sig_hex));
    }

    #[test]
    fn test_authorization_header_roundtrip() {
        let keypair = UserKeypair::generate();
        let sdp = "v=0\r\no=- 123 456 IN IP4 127.0.0.1\r\n";
        let (_, header) = build_authorization_header(&keypair, sdp);
        let verified_addr = verify_authorization_header(&header, sdp).expect("Verification failed");
        assert_eq!(verified_addr, keypair.public_key_address());
    }

    #[test]
    fn test_user_address_generation_from_time() {
        let addr1 = UserAddress::generate_from_time();
        let addr2 = UserAddress::generate_from_time();

        assert_eq!(addr1.id.len(), 64);
        assert_eq!(addr2.id.len(), 64);
        assert_ne!(addr1, addr2);
    }

    #[test]
    fn test_user_address_bytes_roundtrip() {
        let addr = UserAddress::generate_from_time();
        let bytes = addr.to_bytes();
        let reconstructed = UserAddress::from_bytes(bytes);

        assert_eq!(addr, reconstructed);
    }

    #[test]
    fn test_user_address_short_id() {
        let addr = UserAddress::new("a1b2c3d4e5f607080900");
        assert_eq!(addr.short_id(), "a1b2c3d4e5f6");
    }

    #[test]
    fn test_user_address_serde() {
        let addr = UserAddress::generate_from_time();
        let json_str = serde_json::to_string(&addr).expect("Failed to serialize");
        let deserialized: UserAddress =
            serde_json::from_str(&json_str).expect("Failed to deserialize");

        assert_eq!(addr, deserialized);
    }

    #[test]
    fn test_user_address_short_id_short_length() {
        let addr = UserAddress::new("short");
        assert_eq!(addr.short_id(), "short");
    }

    #[test]
    fn test_short_id_to_bytes_roundtrip() {
        let short_addr = UserAddress::new("59ced0911ce1");
        let bytes = short_addr.to_bytes();
        assert_ne!(bytes, [0u8; 32]);
        assert_eq!(&bytes[..6], &[0x59, 0xce, 0xd0, 0x91, 0x1c, 0xe1]);
        let re_addr = UserAddress::from_bytes(bytes);
        assert!(re_addr.id.starts_with("59ced0911ce1"));
    }

    #[test]
    fn test_user_address_from_str_and_display() {
        let addr_str = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let parsed: UserAddress = addr_str.parse().unwrap();
        assert_eq!(parsed.to_string(), addr_str);
        assert_eq!(parsed.id, addr_str);
    }

    #[test]
    fn test_verify_secp256k1_authorization_header() {
        use k256::schnorr::SigningKey;
        use k256::schnorr::signature::Signer;
        use rand::thread_rng;

        let signing_key = SigningKey::random(&mut thread_rng());
        let verifying_key = signing_key.verifying_key();
        let pubkey_bytes = verifying_key.to_bytes();
        let pubkey_hex = hex::encode(pubkey_bytes);

        let sdp = "v=0\r\no=- 123 456 IN IP4 127.0.0.1\r\n";
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let payload = format!("{}:{}", timestamp, sdp);

        let signature: k256::schnorr::Signature = signing_key.sign(payload.as_bytes());
        let sig_hex = hex::encode(signature.to_bytes());

        let header = format!("WP-Secp256k1 {}:{}:{}", pubkey_hex, timestamp, sig_hex);

        let verified_addr = verify_secp256k1_authorization_header(&header, sdp)
            .expect("Schnorr verification should pass");
        assert_eq!(verified_addr.id, pubkey_hex);
    }

    #[test]
    fn test_nostr_keypair_and_bech32() {
        let hex_secret = "3bf0e6984b71239c4a86161427a206a1ed93ee14e04ed96f2a893339f4ad1600";
        let keypair = UserKeypair::from_nostr_key(hex_secret).expect("Failed to parse hex secret");
        assert!(keypair.is_secp256k1());

        let sdp = "v=0\r\no=- 123 456 IN IP4 127.0.0.1\r\n";
        let (_, header) = build_authorization_header(&keypair, sdp);
        assert!(header.starts_with("WP-Secp256k1 "));

        let verified_addr = verify_secp256k1_authorization_header(&header, sdp)
            .expect("Verification of generated WP-Secp256k1 header failed");
        assert_eq!(verified_addr, keypair.public_key_address());

        // Test Bech32 nsec encoding and decoding roundtrip
        let secret_bytes = hex::decode(hex_secret).unwrap();
        let nsec_encoded = encode_bech32("nsec", &secret_bytes).expect("Failed to encode nsec");
        assert!(nsec_encoded.starts_with("nsec1"));

        let nostr_kp = UserKeypair::from_nostr_key(&nsec_encoded).expect("Failed to parse nsec");
        assert!(nostr_kp.is_secp256k1());
        assert_eq!(nostr_kp.public_key_address(), keypair.public_key_address());
    }

    #[test]
    fn test_authorization_header_expired_timestamps() {
        let keypair = UserKeypair::generate();
        let sdp = "v=0\r\no=- 123 456 IN IP4 127.0.0.1\r\n";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cache = AntiReplayCache::new();

        // Expired in past (+301s drift)
        let past_ts = now.saturating_sub(301);
        let payload_past = format!("{}:{}", past_ts, sdp);
        let sig_past = keypair.sign(payload_past.as_bytes());
        let header_past = format!(
            "WP-Ed25519 {}:{}:{}",
            keypair.public_key_address().id,
            past_ts,
            sig_past
        );
        let err_past =
            verify_authorization_header_with_cache(&header_past, sdp, &cache).unwrap_err();
        assert!(matches!(err_past, AuthError::TimestampDrift { .. }));

        // Expired in future (-301s drift)
        let future_ts = now + 301;
        let payload_future = format!("{}:{}", future_ts, sdp);
        let sig_future = keypair.sign(payload_future.as_bytes());
        let header_future = format!(
            "WP-Ed25519 {}:{}:{}",
            keypair.public_key_address().id,
            future_ts,
            sig_future
        );
        let err_future =
            verify_authorization_header_with_cache(&header_future, sdp, &cache).unwrap_err();
        assert!(matches!(err_future, AuthError::TimestampDrift { .. }));

        // Valid boundary (-300s drift)
        let valid_past_ts = now.saturating_sub(300);
        let payload_valid = format!("{}:{}", valid_past_ts, sdp);
        let sig_valid = keypair.sign(payload_valid.as_bytes());
        let header_valid = format!(
            "WP-Ed25519 {}:{}:{}",
            keypair.public_key_address().id,
            valid_past_ts,
            sig_valid
        );
        assert!(verify_authorization_header_with_cache(&header_valid, sdp, &cache).is_ok());
    }

    #[test]
    fn test_authorization_header_tampered_sdp() {
        let keypair = UserKeypair::generate();
        let original_sdp = "v=0\r\no=- 123 456 IN IP4 127.0.0.1\r\n";
        let tampered_sdp = "v=0\r\no=- 999 999 IN IP4 10.0.0.1\r\n";

        let (_, header) = build_authorization_header(&keypair, original_sdp);
        let cache = AntiReplayCache::new();

        let err =
            verify_authorization_header_with_cache(&header, tampered_sdp, &cache).unwrap_err();
        assert_eq!(err, AuthError::InvalidEd25519Signature);
    }

    #[test]
    fn test_authorization_header_anti_replay() {
        let keypair = UserKeypair::generate();
        let sdp = "v=0\r\no=- 123 456 IN IP4 127.0.0.1\r\n";
        let (_, header) = build_authorization_header(&keypair, sdp);
        let cache = AntiReplayCache::new();

        // First attempt: OK
        assert!(verify_authorization_header_with_cache(&header, sdp, &cache).is_ok());

        // Replay attempt: Rejected
        let err = verify_authorization_header_with_cache(&header, sdp, &cache).unwrap_err();
        assert_eq!(err, AuthError::ReplayDetected);
    }

    #[test]
    fn test_verify_any_authorization_header() {
        let cache = AntiReplayCache::new();
        let ed_kp = UserKeypair::generate();
        let sdp = "v=0\r\no=- 123 456 IN IP4 127.0.0.1\r\n";
        let (_, ed_hdr) = build_authorization_header(&ed_kp, sdp);
        assert_eq!(
            verify_any_authorization_header_with_cache(&ed_hdr, sdp, &cache).unwrap(),
            ed_kp.public_key_address()
        );

        let secp_kp = UserKeypair::from_nostr_key(
            "3bf0e6984b71239c4a86161427a206a1ed93ee14e04ed96f2a893339f4ad1600",
        )
        .unwrap();
        let (_, secp_hdr) = build_authorization_header(&secp_kp, sdp);
        assert_eq!(
            verify_any_authorization_header_with_cache(&secp_hdr, sdp, &cache).unwrap(),
            secp_kp.public_key_address()
        );

        let err = verify_any_authorization_header_with_cache("WP-Unknown 123:456:789", sdp, &cache)
            .unwrap_err();
        assert!(matches!(err, AuthError::InvalidScheme(_)));
    }

    #[test]
    fn test_secret_key_non_transmission_in_network_payloads() {
        let secp_hex = "3bf0e6984b71239c4a86161427a206a1ed93ee14e04ed96f2a893339f4ad1600";
        let nostr_kp = UserKeypair::from_nostr_key(secp_hex).unwrap();
        let ed_kp = UserKeypair::generate();
        let sdp = "v=0\r\no=- 12345 67890 IN IP4 127.0.0.1\r\ns=WebPhone\r\n";

        let (_, nostr_header) = build_authorization_header(&nostr_kp, sdp);
        let (_, ed_header) = build_authorization_header(&ed_kp, sdp);

        // Ensure private key string or hex bytes are NEVER included in HTTP authorization headers
        assert!(!nostr_header.contains(secp_hex));
        let ed_secret_hex = hex::encode(ed_kp.to_bytes());
        assert!(!ed_header.contains(&ed_secret_hex));

        // Ensure SDP payload does not leak keypair secret bytes
        assert!(!sdp.contains(secp_hex));
        assert!(!sdp.contains(&ed_secret_hex));
    }

    #[test]
    fn test_prefix_and_zero_trimming_matching_rejected() {
        let addr1 =
            UserAddress::new("1111110000000000000000000000000000000000000000000000000000000000");
        let addr2 = UserAddress::new("111111");
        let addr3 =
            UserAddress::new("1111110000000000000000000000000000000000000000000000000000000001");

        // Exact match should succeed
        assert!(addr1.matches_prefix(&addr1));

        // Prefix and zero-trimmed matches MUST fail for authorization equality
        assert!(!addr1.matches_prefix(&addr2));
        assert!(!addr2.matches_prefix(&addr1));
        assert!(!addr1.matches_prefix(&addr3));
    }

    #[test]
    fn test_user_address_from_room_id() {
        // Arbitrary non-hex room name strings should produce valid 64-char hex addresses
        let room_a = UserAddress::from_room_id("my-awesome-room");
        let room_b = UserAddress::from_room_id("lobby");

        assert_eq!(room_a.id.len(), 64);
        assert_eq!(room_b.id.len(), 64);
        assert_ne!(room_a, room_b);

        // to_bytes() should NOT produce all-zeros for arbitrary room strings
        let bytes_a = room_a.to_bytes();
        assert_ne!(bytes_a, [0u8; 32]);
        assert_eq!(UserAddress::from_bytes(bytes_a), room_a);

        // Exact 64-character hex strings should be preserved
        let hex_id = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let preserved = UserAddress::from_room_id(hex_id);
        assert_eq!(preserved.id, hex_id);

        // Host-key cryptographic room derivation (WPIP-08 §1)
        let keypair = UserKeypair::generate();
        let host_pub = keypair.public_key_address();
        let host_room = UserAddress::from_room_id_and_host("general", &host_pub);
        assert_eq!(host_room.id.len(), 64);
        assert_ne!(host_room, room_a);
    }
}
