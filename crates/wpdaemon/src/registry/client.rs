//! Client 1-to-1 registration and call session management sub-module.

use super::{AddressSearchResult, ClientRegistry, matches_address, matches_address_prefix};
use std::sync::Arc;
use webrtc::peer_connection::PeerConnection;
use wpapi::UserAddress;

impl ClientRegistry {
    /// Register a newly connected client.
    pub fn register_client(
        &mut self,
        client_id: u64,
        user_address: UserAddress,
        peer_connection: Arc<dyn PeerConnection>,
    ) {
        let old_cids: Vec<u64> = self
            .addresses
            .iter()
            .filter_map(|(&cid, addr)| {
                if cid != client_id && addr.id == user_address.id {
                    Some(cid)
                } else {
                    None
                }
            })
            .collect();
        for old_cid in old_cids {
            self.unregister_client(old_cid);
        }

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
    pub fn find_client_by_address(&self, target_key: &UserAddress) -> AddressSearchResult {
        let clean_id = target_key.id.trim_end_matches('0');
        if clean_id.len() < 12 && target_key.id.len() < 12 {
            return AddressSearchResult::NotFound;
        }

        let mut matches = Vec::new();
        for (&cid, addr) in self.addresses.iter() {
            if matches_address_prefix(addr, target_key) {
                matches.push(cid);
            }
        }
        match matches.len() {
            1 => AddressSearchResult::Found(matches[0]),
            0 => AddressSearchResult::NotFound,
            _ => AddressSearchResult::Ambiguous,
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

        if let AddressSearchResult::Found(t_cid) = target_cid {
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

    /// Get sorted and deduplicated list of registered user addresses.
    pub fn get_registered_addresses(&self) -> Vec<UserAddress> {
        let mut addrs: Vec<UserAddress> = self.addresses.values().cloned().collect();
        addrs.sort_by(|a, b| a.id.cmp(&b.id));
        addrs.dedup();
        addrs
    }
}
