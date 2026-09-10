//! End-to-End Integration Test Suite for web-phone workspace.
//!
//! Validates multi-crate interaction between `wpapi`, `wpdaemon`, and protocol codecs.

use wpapi::address::{UserAddress, UserKeypair};
use wpapi::audio::AudioRingBuffer;
use wpapi::protocol::{CODEC_OPUS, ProtocolPacket};
use wpdaemon::registry::{AddressSearchResult, ClientRegistry};

#[test]
fn test_e2e_address_generation_and_bech32_formatting() {
    let keypair = UserKeypair::generate();
    let addr = keypair.user_address();
    assert_eq!(addr.id.len(), 64);

    let bech32_str = addr.to_bech32().expect("Bech32 encoding failed");
    assert!(bech32_str.starts_with("wp1"));

    let decoded_addr = UserAddress::from_bech32(&bech32_str).expect("Bech32 decoding failed");
    assert_eq!(addr, decoded_addr);
}

#[test]
fn test_e2e_audio_ring_buffer_pipeline() {
    let ring = AudioRingBuffer::new(2048);
    assert_eq!(ring.len(), 0);

    // Simulate 480 audio samples (10ms Opus frame at 48kHz)
    let pcm_frame: Vec<f32> = (0..480).map(|i| (i as f32 * 0.01).sin()).collect();
    ring.push_slice(&pcm_frame);
    assert_eq!(ring.len(), 480);

    let mut popped = Vec::new();
    while let Some(sample) = ring.pop() {
        popped.push(sample);
    }
    assert_eq!(popped.len(), 480);
    assert_eq!(ring.len(), 0);
}

#[test]
fn test_e2e_protocol_packet_roundtrips() {
    let caller = UserAddress::generate_from_time();
    let target = UserAddress::generate_from_time();

    let call_req = ProtocolPacket::CallRequest {
        caller_id: 101,
        caller_address: caller.clone(),
    };
    let encoded = call_req.encode();
    let decoded = ProtocolPacket::decode(&encoded).expect("Decode CallRequest failed");
    assert_eq!(decoded, call_req);

    let client_audio = ProtocolPacket::ClientTargetedAudio {
        target_address: target.clone(),
        codec_id: CODEC_OPUS,
        audio_data: vec![0xDE, 0xAD, 0xBE, 0xEF],
    };
    let audio_bytes = client_audio.encode();
    let decoded_audio = ProtocolPacket::decode(&audio_bytes).expect("Decode ClientTargetedAudio failed");
    assert_eq!(decoded_audio, client_audio);
}

#[test]
fn test_e2e_daemon_registry_short_id_resolution() {
    let mut registry = ClientRegistry::new();
    let addr = UserAddress::new("abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890");

    registry.addresses.insert(1, addr.clone());

    // Short ID search (12 chars)
    let short_search = UserAddress::new("abcdef123456");
    assert_eq!(
        registry.find_client_by_address(&short_search),
        AddressSearchResult::Found(1)
    );

    // Ambiguous prefix search
    let addr2 = UserAddress::new("abcdef1234569999999999999999999999999999999999999999999999999999");
    registry.addresses.insert(2, addr2);

    assert_eq!(
        registry.find_client_by_address(&short_search),
        AddressSearchResult::Ambiguous
    );
}
