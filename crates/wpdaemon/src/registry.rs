//! Client registry and daemon state management.
//!
//! Provides a thread-safe, unified `ClientRegistry` structure protected by a single `RwLock`
//! to prevent deadlock and race conditions during client management and call routing.

pub mod client;
pub mod room;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use webrtc::data_channel::DataChannel;
use webrtc::peer_connection::PeerConnection;
use wpapi::UserAddress;

/// Global unified client registry protected by a single RwLock.
pub static CLIENT_REGISTRY: LazyLock<RwLock<ClientRegistry>> =
    LazyLock::new(|| RwLock::new(ClientRegistry::default()));

/// Result of searching for a client by address or Short ID prefix (WPIP-02 Section 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressSearchResult {
    /// Exactly one client matches the target address or prefix.
    Found(u64),
    /// No client matches the target address or prefix.
    NotFound,
    /// Multiple active clients match the Short ID prefix (Ambiguous match).
    Ambiguous,
}

/// Helper to check if address matches room key or peer target using exact identity equality (Issue #37).
pub fn matches_address(addr: &UserAddress, key: &UserAddress) -> bool {
    addr.id == key.id
}

/// Helper to check if address matches target key using prefix match (min 12 chars) for address resolution / routing (WPIP-02 Section 5).
pub fn matches_address_prefix(addr: &UserAddress, key: &UserAddress) -> bool {
    if addr.id == key.id {
        return true;
    }
    // Direct Short ID input (12..63 characters)
    if key.id.len() >= 12 && key.id.len() < 64 && addr.id.starts_with(&key.id) {
        return true;
    }
    // Wire zero-padded Short ID (64 chars total: >=12 hex prefix + trailing zero padding)
    if key.id.len() == 64 && addr.id.len() == 64 {
        let clean_key = key.id.trim_end_matches('0');
        if clean_key.len() >= 12 && clean_key.len() < 64 && addr.id.starts_with(clean_key) {
            return true;
        }
    }
    false
}

/// Unified registry for managing all WebRTC client state, routing, and call approvals.
#[derive(Default)]
pub struct ClientRegistry {
    /// Active WebRTC peer connections (client_id -> Arc<dyn PeerConnection>).
    pub peer_connections: HashMap<u64, Arc<dyn PeerConnection>>,
    /// Registered client addresses (client_id -> UserAddress).
    pub addresses: HashMap<u64, UserAddress>,
    /// Registered client target addresses (client_id -> target UserAddress).
    pub targets: HashMap<u64, UserAddress>,
    /// Active WebRTC client DataChannels (client_id -> Arc<dyn DataChannel>).
    pub data_channels: HashMap<u64, Arc<dyn DataChannel>>,
    /// Approved calls (target_client_id -> Vec<caller_UserAddress>).
    pub approved_calls: HashMap<u64, Vec<UserAddress>>,
    /// Rejected calls (target_client_id -> Vec<caller_UserAddress>).
    pub rejected_calls: HashMap<u64, Vec<UserAddress>>,
    /// Tracked call request notifications (target_client_id -> Vec<caller_client_id>).
    pub notified_requests: HashMap<u64, Vec<u64>>,
    /// Group room members (room_address -> Vec<client_id>).
    pub room_members: HashMap<UserAddress, Vec<u64>>,
    /// Active room speaker energy tracking (room_address -> HashMap<client_id, (energy, Instant)>).
    pub room_speakers: HashMap<UserAddress, HashMap<u64, (f64, std::time::Instant)>>,
    /// Track last pong response time (client_id -> Instant).
    pub last_pong: HashMap<u64, std::time::Instant>,
    /// Track client connection timestamp (client_id -> Instant).
    pub connected_at: HashMap<u64, std::time::Instant>,
    /// Cache of previous Top-K active speakers per room to diff speaker list changes.
    pub prev_top_k_speakers: HashMap<UserAddress, Vec<UserAddress>>,
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wpip08_room_lifecycle_and_top_k_speakers() {
        let mut registry = ClientRegistry::new();
        let room_addr = UserAddress::generate_from_time();

        let client1_addr = UserAddress::generate_from_time();
        let client2_addr = UserAddress::generate_from_time();
        let client3_addr = UserAddress::generate_from_time();
        let client4_addr = UserAddress::generate_from_time();

        registry.addresses.insert(1, client1_addr);
        registry.addresses.insert(2, client2_addr);
        registry.addresses.insert(3, client3_addr);
        registry.addresses.insert(4, client4_addr);

        assert_eq!(registry.join_room(1, room_addr.clone()), 1);
        assert_eq!(registry.join_room(2, room_addr.clone()), 2);
        assert_eq!(registry.join_room(3, room_addr.clone()), 3);
        assert_eq!(registry.join_room(4, room_addr.clone()), 4);

        assert_eq!(registry.get_room_member_ids(&room_addr), vec![1, 2, 3, 4]);

        // Evaluate audio energy for 4 clients (Top K=3 limit check)
        let (is_top1, _, _) = registry.update_speaker_energy(&room_addr, 1, 0.9);
        let (is_top2, _, _) = registry.update_speaker_energy(&room_addr, 2, 0.8);
        let (is_top3, _, _) = registry.update_speaker_energy(&room_addr, 3, 0.7);
        let (is_top4, _, _) = registry.update_speaker_energy(&room_addr, 4, 0.1);

        assert!(is_top1);
        assert!(is_top2);
        assert!(is_top3);
        assert!(
            !is_top4,
            "Client 4 with energy 0.1 must be filtered out by Top-3 SFU limit"
        );

        // Leave room
        assert_eq!(registry.leave_room(4, &room_addr), 3);
        assert_eq!(registry.get_room_member_ids(&room_addr), vec![1, 2, 3]);
    }

    #[test]
    fn test_room_member_limit() {
        let mut registry = ClientRegistry::new();
        let room_addr = UserAddress::generate_from_time();

        assert_eq!(
            registry
                .join_room_with_limit(1, room_addr.clone(), 2)
                .unwrap(),
            1
        );
        assert_eq!(
            registry
                .join_room_with_limit(2, room_addr.clone(), 2)
                .unwrap(),
            2
        );
        assert!(registry.join_room_with_limit(3, room_addr, 2).is_err());
    }

    #[test]
    fn test_wpip09_keep_alive_and_stale_detection() {
        let mut registry = ClientRegistry::new();
        let client_id = 777;

        let past = std::time::Instant::now() - std::time::Duration::from_secs(35);
        registry.last_pong.insert(client_id, past);

        let stale = registry.get_stale_clients(30);
        assert_eq!(stale, vec![client_id]);

        // Client responds with pong
        registry.update_last_pong(client_id);
        let stale_after_pong = registry.get_stale_clients(30);
        assert!(stale_after_pong.is_empty());
    }

    #[test]
    fn test_find_client_by_short_id_prefix() {
        let mut registry = ClientRegistry::new();
        let full_addr = UserAddress::new(
            "39f8adc7bf93a9a611f6cc7a472ec242328d3a2f86274eb098323634db34e829".to_string(),
        );
        registry.addresses.insert(1, full_addr);

        let short_target = UserAddress::new("39f8adc7bf93".to_string());
        let found = registry.find_client_by_address(&short_target);
        assert_eq!(found, AddressSearchResult::Found(1));

        let zero_padded_target = UserAddress::new(
            "39f8adc7bf930000000000000000000000000000000000000000000000000000".to_string(),
        );
        assert_eq!(
            registry.find_client_by_address(&zero_padded_target),
            AddressSearchResult::Found(1)
        );

        let invalid_short = UserAddress::new("39f8adc7".to_string());
        assert_eq!(
            registry.find_client_by_address(&invalid_short),
            AddressSearchResult::NotFound
        );

        // Ambiguous match test (WPIP-02 Section 5)
        let full_addr2 = UserAddress::new(
            "39f8adc7bf93ffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string(),
        );
        registry.addresses.insert(2, full_addr2);
        assert_eq!(
            registry.find_client_by_address(&short_target),
            AddressSearchResult::Ambiguous
        );
    }

    #[tokio::test]
    async fn test_register_client_evicts_duplicate_address() {
        use webrtc::peer_connection::{PeerConnectionBuilder, PeerConnectionEventHandler};

        struct DummyHandler;
        impl PeerConnectionEventHandler for DummyHandler {}

        let mut registry = ClientRegistry::new();
        let addr = UserAddress::generate_from_time();

        let pc1: Arc<dyn webrtc::peer_connection::PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_handler(Arc::new(DummyHandler))
                .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
                .build()
                .await
                .unwrap(),
        );
        let pc2: Arc<dyn webrtc::peer_connection::PeerConnection> = Arc::new(
            PeerConnectionBuilder::new()
                .with_handler(Arc::new(DummyHandler))
                .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
                .build()
                .await
                .unwrap(),
        );

        registry.register_client(1, addr.clone(), pc1);
        assert_eq!(
            registry.find_client_by_address(&addr),
            AddressSearchResult::Found(1)
        );

        // New client registers with same address (e.g. reconnect after crash)
        registry.register_client(2, addr.clone(), pc2);
        assert_eq!(
            registry.find_client_by_address(&addr),
            AddressSearchResult::Found(2)
        );
        assert!(!registry.addresses.contains_key(&1));
    }
}
