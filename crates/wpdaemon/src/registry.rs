//! Client registry and daemon state management.
//!
//! Provides a thread-safe, unified `ClientRegistry` structure protected by a single `RwLock`
//! to prevent deadlock and race conditions during client management and call routing.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use webrtc::data_channel::DataChannel;
use webrtc::peer_connection::PeerConnection;
use wpapi::UserAddress;

/// Global unified client registry protected by a single RwLock.
pub static CLIENT_REGISTRY: LazyLock<RwLock<ClientRegistry>> =
    LazyLock::new(|| RwLock::new(ClientRegistry::default()));

/// Helper to check if address matches room key (exact match or prefix match, handling zero-padded Short IDs).
pub fn matches_address(addr: &UserAddress, key: &UserAddress) -> bool {
    if addr.id == key.id {
        return true;
    }
    if key.id.len() >= 12 && addr.id.starts_with(&key.id) {
        return true;
    }
    if addr.id.len() >= 12 && key.id.starts_with(&addr.id) {
        return true;
    }
    let key_clean = key.id.trim_end_matches('0');
    if key_clean.len() >= 12 && addr.id.starts_with(key_clean) {
        return true;
    }
    let addr_clean = addr.id.trim_end_matches('0');
    if addr_clean.len() >= 12 && key.id.starts_with(addr_clean) {
        return true;
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
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a newly connected client.
    pub fn register_client(
        &mut self,
        client_id: u64,
        user_address: UserAddress,
        peer_connection: Arc<dyn PeerConnection>,
    ) {
        let now = std::time::Instant::now();
        self.peer_connections.insert(client_id, peer_connection);
        self.addresses.insert(client_id, user_address);
        self.connected_at.insert(client_id, now);
        self.last_pong.insert(client_id, now);
    }

    /// Unregister a disconnected client and cleanup all associated state.
    pub fn unregister_client(&mut self, client_id: u64) {
        self.peer_connections.remove(&client_id);
        self.addresses.remove(&client_id);
        self.targets.remove(&client_id);
        self.data_channels.remove(&client_id);
        self.approved_calls.remove(&client_id);
        self.rejected_calls.remove(&client_id);
        self.notified_requests.remove(&client_id);
        self.last_pong.remove(&client_id);
        self.connected_at.remove(&client_id);

        for members in self.room_members.values_mut() {
            members.retain(|&cid| cid != client_id);
        }
        for speakers in self.room_speakers.values_mut() {
            speakers.remove(&client_id);
        }
        crate::rate_limit::DATACHANNEL_RATE_LIMITER.remove_client(client_id);
    }

    /// Find client ID matching a given UserAddress (exact or prefix match per WPIP-02 Section 5).
    pub fn find_client_by_address(&self, target_key: &UserAddress) -> Option<u64> {
        let clean_id = target_key.id.trim_end_matches('0');
        if clean_id.len() < 12 && target_key.id.len() < 12 {
            return None;
        }

        let mut matches = Vec::new();
        for (&cid, addr) in self.addresses.iter() {
            if matches_address(addr, target_key) {
                matches.push(cid);
            }
        }
        // Ambiguity Rule: MUST NOT select arbitrarily if multiple matches exist
        if matches.len() == 1 {
            Some(matches[0])
        } else {
            None
        }
    }

    /// Check if a call from `caller_addr` to `target_id` is approved.
    pub fn is_call_approved(&self, target_id: u64, caller_addr: &UserAddress) -> bool {
        if let Some(list) = self.approved_calls.get(&target_id) {
            list.iter().any(|a| matches_address(a, caller_addr))
        } else {
            false
        }
    }

    /// Check if a call from `caller_addr` to `target_id` is rejected.
    pub fn is_call_rejected(&self, target_id: u64, caller_addr: &UserAddress) -> bool {
        if let Some(list) = self.rejected_calls.get(&target_id) {
            list.iter().any(|a| matches_address(a, caller_addr))
        } else {
            false
        }
    }

    /// Mark a call from `caller_addr` to `target_id` as approved.
    pub fn mark_call_approved(&mut self, target_id: u64, caller_addr: UserAddress) {
        let list = self.approved_calls.entry(target_id).or_default();
        if !list.iter().any(|a| matches_address(a, &caller_addr)) {
            list.push(caller_addr);
        }
    }

    /// Mark a call from `caller_addr` to `target_id` as rejected.
    pub fn mark_call_rejected(&mut self, target_id: u64, caller_addr: UserAddress) {
        let list = self.rejected_calls.entry(target_id).or_default();
        if !list.iter().any(|a| matches_address(a, &caller_addr)) {
            list.push(caller_addr);
        }
    }

    /// Check if target_id has already been notified about caller_id's request.
    pub fn has_been_notified(&self, target_id: u64, caller_id: u64) -> bool {
        if let Some(list) = self.notified_requests.get(&target_id) {
            list.contains(&caller_id)
        } else {
            false
        }
    }

    /// Mark target_id as notified about caller_id's request.
    pub fn mark_notified(&mut self, target_id: u64, caller_id: u64) {
        let list = self.notified_requests.entry(target_id).or_default();
        if !list.contains(&caller_id) {
            list.push(caller_id);
        }
    }

    /// Clear call approval state for a given client and target UserAddress.
    pub fn clear_call_session(&mut self, client_id: u64, target_key: &UserAddress) {
        let target_cid = self.find_client_by_address(target_key);
        let my_addr = self.addresses.get(&client_id).cloned();

        if let Some(list) = self.approved_calls.get_mut(&client_id) {
            list.retain(|a| !matches_address(a, target_key));
        }

        if let Some(t_cid) = target_cid {
            if let Some(list) = self.approved_calls.get_mut(&t_cid)
                && let Some(ref addr) = my_addr
            {
                list.retain(|a| !matches_address(a, addr));
            }
            if let Some(list) = self.notified_requests.get_mut(&t_cid) {
                list.retain(|&cid| cid != client_id);
            }
            if let Some(list) = self.notified_requests.get_mut(&client_id) {
                list.retain(|&cid| cid != t_cid);
            }
        }
    }

    /// Check if client_id belongs to the room identified by room_key.
    pub fn is_client_in_room(&self, client_id: u64, room_key: &UserAddress) -> bool {
        let my_addr = self.addresses.get(&client_id);
        let my_target = self.targets.get(&client_id);

        if let Some(addr) = my_addr
            && matches_address(addr, room_key)
        {
            return true;
        }
        if let Some(target) = my_target
            && matches_address(target, room_key)
        {
            return true;
        }
        false
    }

    /// Count participants in room identified by `room_key`.
    pub fn get_participant_count_for_room(&self, room_key: &UserAddress) -> usize {
        let mut count = 0;
        for (&cid, addr) in self.addresses.iter() {
            let target = self.targets.get(&cid);
            if matches_address(addr, room_key)
                || target.is_some_and(|t| matches_address(t, room_key))
            {
                count += 1;
            }
        }
        count
    }

    /// Check if client_id is already part of the call with target_key.
    pub fn is_client_in_same_call(&self, client_id: u64, target_key: &UserAddress) -> bool {
        if self.is_client_in_room(client_id, target_key) {
            return true;
        }

        let my_addr = match self.addresses.get(&client_id) {
            Some(a) => a,
            None => return false,
        };

        for (&cid, addr) in self.addresses.iter() {
            if matches_address(addr, target_key)
                && let Some(other_target) = self.targets.get(&cid)
                && matches_address(other_target, my_addr)
            {
                return true;
            }
        }
        false
    }

    /// Check if room or target user is already in a call with maximum allowed participants (2).
    pub fn is_room_or_target_full(&self, target_key: &UserAddress) -> bool {
        if self.get_participant_count_for_room(target_key) >= 2 {
            return true;
        }

        for (&cid, addr) in self.addresses.iter() {
            if matches_address(addr, target_key)
                && let Some(other_room) = self.targets.get(&cid)
                && !matches_address(other_room, target_key)
                && self.get_participant_count_for_room(other_room) >= 2
            {
                return true;
            }
        }
        false
    }

    /// Get sorted and deduplicated list of registered user addresses.
    pub fn get_registered_addresses(&self) -> Vec<UserAddress> {
        let mut addrs: Vec<UserAddress> = self.addresses.values().cloned().collect();
        addrs.sort_by(|a, b| a.id.cmp(&b.id));
        addrs.dedup();
        addrs
    }

    /// Join client to a group room with member limit check, returning updated room participant count.
    pub fn join_room_with_limit(
        &mut self,
        client_id: u64,
        room_address: UserAddress,
        max_members: usize,
    ) -> Result<u32, &'static str> {
        let mut target_key = room_address.clone();
        for key in self.room_members.keys() {
            if matches_address(key, &room_address) {
                target_key = key.clone();
                break;
            }
        }
        let members = self.room_members.entry(target_key).or_default();
        if !members.contains(&client_id) {
            if members.len() >= max_members {
                return Err("Room member limit reached");
            }
            members.push(client_id);
        }
        Ok(members.len() as u32)
    }

    /// Join client to a group room, returning updated room participant count.
    pub fn join_room(&mut self, client_id: u64, room_address: UserAddress) -> u32 {
        self.join_room_with_limit(client_id, room_address.clone(), 50)
            .unwrap_or_else(|_| {
                let mut target_key = room_address.clone();
                for key in self.room_members.keys() {
                    if matches_address(key, &room_address) {
                        target_key = key.clone();
                        break;
                    }
                }
                self.room_members.get(&target_key).map_or(0, |m| m.len()) as u32
            })
    }

    /// Leave room for a client, returning remaining participant count.
    pub fn leave_room(&mut self, client_id: u64, room_address: &UserAddress) -> u32 {
        let mut count = 0;
        let mut target_room_key = None;
        for (room_key, members) in self.room_members.iter_mut() {
            if matches_address(room_key, room_address) {
                members.retain(|&cid| cid != client_id);
                count = members.len() as u32;
                target_room_key = Some(room_key.clone());
                break;
            }
        }
        if let Some(ref r_key) = target_room_key
            && let Some(speakers) = self.room_speakers.get_mut(r_key)
        {
            speakers.remove(&client_id);
        }
        count
    }

    /// Get all client IDs registered in a room matching room_address.
    pub fn get_room_member_ids(&self, room_address: &UserAddress) -> Vec<u64> {
        for (room_key, members) in self.room_members.iter() {
            if matches_address(room_key, room_address) {
                return members.clone();
            }
        }
        Vec::new()
    }

    /// Update audio energy level for a client in a room.
    /// Returns tuple `(is_top_k, top_k_user_addresses, speaker_list_changed)`.
    pub fn update_speaker_energy(
        &mut self,
        room_address: &UserAddress,
        client_id: u64,
        energy: f64,
    ) -> (bool, Vec<UserAddress>, bool) {
        let now = std::time::Instant::now();
        let mut room_key = room_address.clone();

        for key in self.room_members.keys() {
            if matches_address(key, room_address) {
                room_key = key.clone();
                break;
            }
        }

        let speakers = self.room_speakers.entry(room_key.clone()).or_default();
        speakers.insert(client_id, (energy, now));

        // Purge speakers quiet/inactive for > 3 seconds
        speakers.retain(|_, (_, last_seen)| now.duration_since(*last_seen).as_secs_f64() <= 3.0);

        // Collect and sort speakers by energy level descending
        let mut speaker_list: Vec<(u64, f64)> = speakers
            .iter()
            .map(|(&cid, (eng, _))| (cid, *eng))
            .collect();
        speaker_list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Select top K=3 active speakers
        let top_k_cids: Vec<u64> = speaker_list.iter().take(3).map(|(cid, _)| *cid).collect();
        let is_top_k = top_k_cids.contains(&client_id);

        let top_k_addrs: Vec<UserAddress> = top_k_cids
            .iter()
            .filter_map(|cid| self.addresses.get(cid).cloned())
            .collect();

        (is_top_k, top_k_addrs, false)
    }

    /// Update last pong response timestamp for client.
    pub fn update_last_pong(&mut self, client_id: u64) {
        self.last_pong.insert(client_id, std::time::Instant::now());
    }

    /// Identify stale clients that have missed pongs for more than `timeout_secs`.
    pub fn get_stale_clients(&self, timeout_secs: u64) -> Vec<u64> {
        let now = std::time::Instant::now();
        let mut stale = Vec::new();

        for (&cid, &last_seen) in self.last_pong.iter() {
            if now.duration_since(last_seen).as_secs() >= timeout_secs {
                stale.push(cid);
            }
        }

        stale
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
}
