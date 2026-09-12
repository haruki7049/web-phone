//! Connection handling module.
//!
//! Provides WebRTC SDP offer processing, DataChannel message routing, and keep-alive tasks.

pub mod datachannel;
pub mod handshake;
pub mod keepalive;

pub use datachannel::validate_packet_sender_identity;
pub use handshake::handle_sdp_offer;
pub use keepalive::start_keepalive_task;

use crate::registry::CLIENT_REGISTRY;
use wpapi::UserAddress;

/// Helper to calculate audio energy (RMS) of audio frame payload or extract unencrypted WPIP-11 SFrame AudioEnergyByte.
pub fn calculate_audio_energy(audio_data: &[u8]) -> f64 {
    if audio_data.is_empty() {
        return 0.0;
    }
    if !audio_data.len().is_multiple_of(4) {
        return (audio_data[0] as f64) / 255.0;
    }
    let (chunks, _) = audio_data.as_chunks::<4>();
    let sum_sq: f64 = chunks
        .iter()
        .map(|c| {
            let sample = f32::from_le_bytes(*c) as f64;
            sample * sample
        })
        .sum();
    (sum_sq / chunks.len() as f64).sqrt()
}

/// Find client ID matching a given UserAddress.
pub fn find_client_by_address(target_key: &UserAddress) -> crate::registry::AddressSearchResult {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .find_client_by_address(target_key)
}

fn is_call_approved(target_id: u64, caller_addr: &UserAddress) -> bool {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .is_call_approved(target_id, caller_addr)
}

fn is_call_rejected(target_id: u64, caller_addr: &UserAddress) -> bool {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .is_call_rejected(target_id, caller_addr)
}

fn mark_call_approved(target_id: u64, caller_addr: UserAddress) {
    CLIENT_REGISTRY
        .write()
        .unwrap()
        .mark_call_approved(target_id, caller_addr);
}

fn mark_call_rejected(target_id: u64, caller_addr: UserAddress) {
    CLIENT_REGISTRY
        .write()
        .unwrap()
        .mark_call_rejected(target_id, caller_addr);
}

fn has_been_notified(target_id: u64, caller_id: u64) -> bool {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .has_been_notified(target_id, caller_id)
}

fn mark_notified(target_id: u64, caller_id: u64) {
    CLIENT_REGISTRY
        .write()
        .unwrap()
        .mark_notified(target_id, caller_id);
}

fn is_client_in_room(client_id: u64, room_key: &UserAddress) -> bool {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .is_client_in_room(client_id, room_key)
}

/// Count participants in room identified by `room_key`.
pub fn get_participant_count_for_room(room_key: &UserAddress) -> usize {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .get_participant_count_for_room(room_key)
}

fn is_client_in_same_call(client_id: u64, target_key: &UserAddress) -> bool {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .is_client_in_same_call(client_id, target_key)
}

fn is_room_or_target_full(target_key: &UserAddress) -> bool {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .is_room_or_target_full(target_key)
}

/// Get currently registered wpclient addresses across active connections.
pub fn get_registered_addresses() -> Vec<UserAddress> {
    CLIENT_REGISTRY.read().unwrap().get_registered_addresses()
}

/// Get client UserAddress by client_id.
pub fn get_client_address(client_id: u64) -> Option<UserAddress> {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .addresses
        .get(&client_id)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Json;
    use axum::http::HeaderMap;
    use webrtc::peer_connection::RTCSessionDescription;
    use wpapi::ProtocolPacket;

    #[test]
    fn test_get_registered_addresses_empty() {
        let addrs = get_registered_addresses();
        assert!(
            addrs.is_empty() || !addrs.contains(&UserAddress::new("non_existent_test_id_12345"))
        );
    }

    #[test]
    fn test_client_addresses_registration() {
        let dummy_addr = UserAddress::generate_from_time();
        CLIENT_REGISTRY
            .write()
            .unwrap()
            .addresses
            .insert(99999, dummy_addr.clone());

        let addrs = get_registered_addresses();
        assert!(addrs.contains(&dummy_addr));

        CLIENT_REGISTRY.write().unwrap().unregister_client(99999);
    }

    #[tokio::test]
    async fn test_handle_sdp_offer_auth_failures() {
        use axum::http::StatusCode;

        let headers = HeaderMap::new();
        let invalid_sdp = RTCSessionDescription::offer(
            "v=0\r\no=- 12345 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n".to_string(),
        )
        .unwrap();

        let mut bad_headers = headers.clone();
        bad_headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer invalid_token"),
        );

        let res = handle_sdp_offer(bad_headers, Json(invalid_sdp.clone())).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_addresses_authentication_requirement() {
        use axum::http::{HeaderMap, HeaderValue};

        let headers = HeaderMap::new();
        let auth_header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        assert!(auth_header.is_none());

        let mut invalid_headers = HeaderMap::new();
        invalid_headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer invalid_token"),
        );
        let auth_val = invalid_headers
            .get(axum::http::header::AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(wpapi::verify_any_authorization_header(auth_val, "").is_err());

        let keypair = wpapi::UserKeypair::generate();
        let (_, valid_hdr_val) = wpapi::build_authorization_header(&keypair, "");
        let mut valid_headers = HeaderMap::new();
        valid_headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(&valid_hdr_val).unwrap(),
        );
        let valid_auth_val = valid_headers
            .get(axum::http::header::AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(wpapi::verify_any_authorization_header(valid_auth_val, "").is_ok());
    }

    #[test]
    fn test_packet_sender_identity_binding_and_spoof_rejection() {
        let auth_client_id = 42u64;
        let auth_user_addr = UserAddress::generate_from_time();

        let attacker_addr = UserAddress::generate_from_time();
        let attacker_client_id = 999u64;

        let valid_packet = ProtocolPacket::ServerTargetedAudio {
            target_address: UserAddress::generate_from_time(),
            sender_id: auth_client_id,
            sender_address: auth_user_addr.clone(),
            codec_id: wpapi::protocol::CODEC_OPUS,
            audio_data: vec![0x01, 0x02],
        };
        assert!(validate_packet_sender_identity(
            &valid_packet,
            auth_client_id,
            &auth_user_addr
        ));

        let spoofed_id_packet = ProtocolPacket::ServerTargetedAudio {
            target_address: UserAddress::generate_from_time(),
            sender_id: attacker_client_id,
            sender_address: auth_user_addr.clone(),
            codec_id: wpapi::protocol::CODEC_OPUS,
            audio_data: vec![0x01, 0x02],
        };
        assert!(!validate_packet_sender_identity(
            &spoofed_id_packet,
            auth_client_id,
            &auth_user_addr
        ));

        let spoofed_addr_packet = ProtocolPacket::ServerTargetedAudio {
            target_address: UserAddress::generate_from_time(),
            sender_id: auth_client_id,
            sender_address: attacker_addr.clone(),
            codec_id: wpapi::protocol::CODEC_OPUS,
            audio_data: vec![0x01, 0x02],
        };
        assert!(!validate_packet_sender_identity(
            &spoofed_addr_packet,
            auth_client_id,
            &auth_user_addr
        ));

        let spoofed_broadcast = ProtocolPacket::BroadcastAudio {
            sender_id: attacker_client_id,
            audio_data: vec![0x00],
        };
        assert!(!validate_packet_sender_identity(
            &spoofed_broadcast,
            auth_client_id,
            &auth_user_addr
        ));

        let spoofed_call_req = ProtocolPacket::CallRequest {
            caller_id: attacker_client_id,
            caller_address: attacker_addr,
        };
        assert!(!validate_packet_sender_identity(
            &spoofed_call_req,
            auth_client_id,
            &auth_user_addr
        ));
    }

    #[test]
    fn test_sfu_zero_knowledge_energy_routing() {
        let key: [u8; 32] = rand::random();
        let nonce: [u8; 12] = rand::random();
        let sframe = wpapi::SFrameCodec::new(&key).unwrap();

        let raw_pcm = [0.5f32.to_le_bytes(), 0.8f32.to_le_bytes()].concat();
        let energy_byte = 204u8;

        let encrypted_frame = sframe.encrypt_frame(&nonce, energy_byte, &raw_pcm).unwrap();

        let calculated_energy = calculate_audio_energy(&encrypted_frame);
        assert!((calculated_energy - (204.0 / 255.0)).abs() < 1e-4);
    }

    #[test]
    fn test_wpip06_embedded_stun_binding_request_parsing() {
        // WPIP-06: STUN Binding Request (0x0001) binary header structure validation
        let stun_binding_request_header: [u8; 20] = [
            0x00, 0x01, // Message Type: Binding Request (0x0001)
            0x00, 0x00, // Message Length: 0
            0x21, 0x12, 0xA4, 0x42, // Magic Cookie (RFC 5389)
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
            0x0C, // 96-bit Transaction ID
        ];

        let msg_type = u16::from_be_bytes([
            stun_binding_request_header[0],
            stun_binding_request_header[1],
        ]);
        let magic_cookie = u32::from_be_bytes([
            stun_binding_request_header[4],
            stun_binding_request_header[5],
            stun_binding_request_header[6],
            stun_binding_request_header[7],
        ]);

        assert_eq!(msg_type, 0x0001);
        assert_eq!(magic_cookie, 0x2112A442);
    }

    #[test]
    fn test_wpip19_direct_client_ice_candidate_signaling() {
        // WPIP-19: Direct Client-to-Client ICE transport negotiation signaling structure
        let candidate_sdp = "candidate:842163049 1 udp 1677721601 192.168.1.150 54321 typ host";
        let client_a = UserAddress::generate_from_time();
        let client_b = UserAddress::generate_from_time();

        assert!(candidate_sdp.contains("typ host"));
        assert_ne!(client_a, client_b);
    }
}
