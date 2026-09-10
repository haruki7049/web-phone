//! Room / SFU group calling registry sub-module.

use super::{ClientRegistry, matches_address, matches_address_prefix};
use wpapi::UserAddress;

impl ClientRegistry {
    /// Check if client_id belongs to the room identified by room_key.
    pub fn is_client_in_room(&self, client_id: u64, room_key: &UserAddress) -> bool {
        let my_addr = self.addresses.get(&client_id);
        let my_target = self.targets.get(&client_id);

        if let Some(addr) = my_addr
            && (matches_address(addr, room_key) || matches_address_prefix(addr, room_key))
        {
            return true;
        }
        if let Some(target) = my_target
            && (matches_address(target, room_key)
                || matches_address_prefix(target, room_key)
                || matches_address_prefix(room_key, target))
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
                || matches_address_prefix(addr, room_key)
                || target.is_some_and(|t| {
                    matches_address(t, room_key)
                        || matches_address_prefix(t, room_key)
                        || matches_address_prefix(room_key, t)
                })
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
            if (matches_address(addr, target_key) || matches_address_prefix(addr, target_key))
                && let Some(other_target) = self.targets.get(&cid)
                && (matches_address(other_target, my_addr)
                    || matches_address_prefix(my_addr, other_target)
                    || matches_address_prefix(other_target, my_addr))
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
            if (matches_address(addr, target_key) || matches_address_prefix(addr, target_key))
                && let Some(other_room) = self.targets.get(&cid)
                && !matches_address(other_room, target_key)
                && !matches_address_prefix(other_room, target_key)
                && self.get_participant_count_for_room(other_room) >= 2
            {
                return true;
            }
        }
        false
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

        let list_changed = match self.prev_top_k_speakers.get(&room_key) {
            Some(prev) => prev != &top_k_addrs,
            None => !top_k_addrs.is_empty(),
        };

        if list_changed {
            self.prev_top_k_speakers
                .insert(room_key, top_k_addrs.clone());
        }

        (is_top_k, top_k_addrs, list_changed)
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
