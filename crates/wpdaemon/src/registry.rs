//! Client registry and daemon state management.
//!
//! Provides a thread-safe, unified `ClientRegistry` structure protected by a single `RwLock`
//! to prevent deadlock and race conditions during client management and call routing.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::RTCPeerConnection;
use wpffi::UserAddress;

/// Global unified client registry protected by a single RwLock.
pub static CLIENT_REGISTRY: LazyLock<RwLock<ClientRegistry>> =
    LazyLock::new(|| RwLock::new(ClientRegistry::default()));

/// Helper to check if address matches room key (exact match or prefix match, handling zero-padded Short IDs).
pub fn matches_address(addr: &UserAddress, key: &UserAddress) -> bool {
    if addr.id == key.id {
        return true;
    }
    let key_clean = key.id.trim_end_matches('0');
    let addr_clean = addr.id.trim_end_matches('0');

    (!key_clean.is_empty() && key_clean.len() <= addr.id.len() && addr.id.starts_with(key_clean))
        || (!addr_clean.is_empty() && addr_clean.len() <= key.id.len() && key.id.starts_with(addr_clean))
}

/// Unified registry for managing all WebRTC client state, routing, and call approvals.
#[derive(Default)]
pub struct ClientRegistry {
    /// Active WebRTC peer connections (client_id -> Arc<RTCPeerConnection>).
    pub peer_connections: HashMap<u64, Arc<RTCPeerConnection>>,
    /// Registered client addresses (client_id -> UserAddress).
    pub addresses: HashMap<u64, UserAddress>,
    /// Registered client target addresses (client_id -> target UserAddress).
    pub targets: HashMap<u64, UserAddress>,
    /// Active WebRTC client DataChannels (client_id -> Arc<RTCDataChannel>).
    pub data_channels: HashMap<u64, Arc<RTCDataChannel>>,
    /// Approved calls (target_client_id -> Vec<caller_UserAddress>).
    pub approved_calls: HashMap<u64, Vec<UserAddress>>,
    /// Rejected calls (target_client_id -> Vec<caller_UserAddress>).
    pub rejected_calls: HashMap<u64, Vec<UserAddress>>,
    /// Tracked call request notifications (target_client_id -> Vec<caller_client_id>).
    pub notified_requests: HashMap<u64, Vec<u64>>,
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
        peer_connection: Arc<RTCPeerConnection>,
    ) {
        self.peer_connections.insert(client_id, peer_connection);
        self.addresses.insert(client_id, user_address);
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
    }

    /// Find client ID matching a given UserAddress (exact or prefix match).
    pub fn find_client_by_address(&self, target_key: &UserAddress) -> Option<u64> {
        for (&cid, addr) in self.addresses.iter() {
            if matches_address(addr, target_key) {
                return Some(cid);
            }
        }
        None
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
}
