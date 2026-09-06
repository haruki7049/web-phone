//! Connection handling module.
//!
//! This module provides the logic for handling WebRTC SDP offer requests,
//! managing client PeerConnections, and routing audio between connected WebRTC clients.

use crate::broadcast::{AUDIO_BROADCAST, AudioMessage};
use crate::config::CONFIGURATION;
use axum::{
    extract::Json,
    http::{HeaderMap, StatusCode},
};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use std::sync::{Arc, LazyLock, Mutex};
use tracing::{error, info, warn};
use wclient::UserAddress;
use webrtc::api::APIBuilder;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

/// Counter for connected clients.
static CLIENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Active WebRTC peer connections.
static PEER_CONNECTIONS: LazyLock<Mutex<HashMap<u64, Arc<RTCPeerConnection>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Registered client addresses (client_id -> UserAddress).
static CLIENT_ADDRESSES: LazyLock<Mutex<HashMap<u64, UserAddress>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Registered client target addresses (client_id -> target UserAddress).
static CLIENT_TARGETS: LazyLock<Mutex<HashMap<u64, UserAddress>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Maximum audio message size in bytes (1MB).
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Helper to check if address matches room key (exact match or prefix match).
fn matches_address(addr: &UserAddress, key: &UserAddress) -> bool {
    addr.id == key.id
        || (!key.id.is_empty() && key.id.len() <= addr.id.len() && addr.id.starts_with(&key.id))
        || (!addr.id.is_empty() && addr.id.len() <= key.id.len() && key.id.starts_with(&addr.id))
}

/// Check if a client belongs to the ad-hoc room identified by room_key.
fn is_client_in_room(client_id: u64, room_key: &UserAddress) -> bool {
    let my_addr = CLIENT_ADDRESSES.lock().unwrap().get(&client_id).cloned();
    let my_target = CLIENT_TARGETS.lock().unwrap().get(&client_id).cloned();

    if let Some(ref addr) = my_addr
        && matches_address(addr, room_key)
    {
        return true;
    }

    if let Some(ref target) = my_target
        && matches_address(target, room_key)
    {
        return true;
    }
    false
}

fn count_participants_inner(
    addrs: &HashMap<u64, UserAddress>,
    targets: &HashMap<u64, UserAddress>,
    room_key: &UserAddress,
) -> usize {
    let mut count = 0;
    for (&cid, addr) in addrs.iter() {
        let target = targets.get(&cid);
        if matches_address(addr, room_key) || target.is_some_and(|t| matches_address(t, room_key)) {
            count += 1;
        }
    }
    count
}

/// Count participants in room identified by `room_key`.
pub fn get_participant_count_for_room(room_key: &UserAddress) -> usize {
    let addrs = CLIENT_ADDRESSES.lock().unwrap();
    let targets = CLIENT_TARGETS.lock().unwrap();
    count_participants_inner(&addrs, &targets, room_key)
}

/// Check if client_id is already part of the call with target_key.
fn is_client_in_same_call(client_id: u64, target_key: &UserAddress) -> bool {
    if is_client_in_room(client_id, target_key) {
        return true;
    }

    let addrs = CLIENT_ADDRESSES.lock().unwrap();
    let targets = CLIENT_TARGETS.lock().unwrap();

    let my_addr = match addrs.get(&client_id) {
        Some(a) => a,
        None => return false,
    };

    // Check if target client (matching target_key) is targeting my_addr
    for (&cid, addr) in addrs.iter() {
        if matches_address(addr, target_key)
            && let Some(other_target) = targets.get(&cid)
            && matches_address(other_target, my_addr)
        {
            return true;
        }
    }
    false
}

/// Check if room or target user is already in a call with maximum allowed participants (2).
fn is_room_or_target_full(target_key: &UserAddress) -> bool {
    let addrs = CLIENT_ADDRESSES.lock().unwrap();
    let targets = CLIENT_TARGETS.lock().unwrap();

    if count_participants_inner(&addrs, &targets, target_key) >= 2 {
        return true;
    }

    for (&cid, addr) in addrs.iter() {
        if matches_address(addr, target_key)
            && let Some(other_room) = targets.get(&cid)
            && !matches_address(other_room, target_key)
            && count_participants_inner(&addrs, &targets, other_room) >= 2
        {
            return true;
        }
    }

    false
}

/// Get currently registered wclient addresses across active connections.
pub fn get_registered_addresses() -> Vec<UserAddress> {
    let mut addrs: Vec<UserAddress> = CLIENT_ADDRESSES.lock().unwrap().values().cloned().collect();
    addrs.sort_by(|a, b| a.id.cmp(&b.id));
    addrs.dedup();
    addrs
}

/// Handle an SDP offer from a WebRTC client.
pub async fn handle_sdp_offer(
    _headers: HeaderMap,
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, (StatusCode, String)> {
    let daemon_config = CONFIGURATION.get().cloned().unwrap_or_default();
    let my_node_id = daemon_config.node_id;

    let api = APIBuilder::new().build();
    let config = RTCConfiguration::default();

    let peer_connection = Arc::new(api.new_peer_connection(config).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create PeerConnection: {}", e),
        )
    })?);

    let client_id = CLIENT_COUNT.fetch_add(1, Ordering::SeqCst);

    // Generate temporary user address derived from UNIX timestamp SHA-256 hash
    let user_address = UserAddress::generate_from_time();

    info!(
        "Client {} connected, assigned temporary SHA-256 user ID: {}",
        client_id, user_address
    );

    PEER_CONNECTIONS
        .lock()
        .unwrap()
        .insert(client_id, Arc::clone(&peer_connection));

    CLIENT_ADDRESSES
        .lock()
        .unwrap()
        .insert(client_id, user_address.clone());

    peer_connection.on_peer_connection_state_change(Box::new(
        move |state: RTCPeerConnectionState| {
            info!("Client {} PeerConnection state: {}", client_id, state);
            if state == RTCPeerConnectionState::Failed
                || state == RTCPeerConnectionState::Closed
                || state == RTCPeerConnectionState::Disconnected
            {
                info!(
                    "Client {} ({}) disconnected",
                    client_id,
                    user_address.short_id()
                );
                PEER_CONNECTIONS.lock().unwrap().remove(&client_id);
                CLIENT_ADDRESSES.lock().unwrap().remove(&client_id);
                CLIENT_TARGETS.lock().unwrap().remove(&client_id);
            }
            Box::pin(async move {})
        },
    ));

    peer_connection.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        let dc_label = dc.label().to_string();
        info!("Client {} created DataChannel: {}", client_id, dc_label);

        let dc_open = Arc::clone(&dc);

        dc.on_open(Box::new(move || {
            let dc_inner = Arc::clone(&dc_open);
            let mut audio_rx = AUDIO_BROADCAST.subscribe();

            Box::pin(async move {
                info!("Client {} DataChannel opened", client_id);

                let my_addr = CLIENT_ADDRESSES
                    .lock()
                    .unwrap()
                    .get(&client_id)
                    .cloned()
                    .unwrap_or_default();

                // Send client ID and registered SHA-256 address assignment message:
                // [0x00, client_id (8 bytes LE), user_address (32 bytes SHA256)]
                let mut init_msg = Vec::with_capacity(41);
                init_msg.push(0x00);
                init_msg.extend_from_slice(&client_id.to_le_bytes());
                init_msg.extend_from_slice(&my_addr.to_bytes());

                if let Err(e) = dc_inner.send(&Bytes::from(init_msg)).await {
                    error!("Failed to send client info to client {}: {}", client_id, e);
                    return;
                }

                // Forward ad-hoc room audio to this client if it belongs to the target room
                tokio::spawn(async move {
                    loop {
                        match audio_rx.recv().await {
                            Ok(audio_msg) => {
                                // Skip loopback to sender
                                if audio_msg.sender_id == client_id {
                                    continue;
                                }

                                let target_room = &audio_msg.target_address;
                                if is_client_in_room(client_id, target_room) {
                                    let sender_addr_bytes = audio_msg
                                        .sender_address
                                        .map(|a| a.to_bytes())
                                        .unwrap_or([0u8; 32]);

                                    // Packet: [0x03, target_address (32b), sender_id (8b LE), sender_address (32b), audio_data...]
                                    let mut packet = Vec::with_capacity(73 + audio_msg.data.len());
                                    packet.push(0x03);
                                    packet.extend_from_slice(&target_room.to_bytes());
                                    packet.extend_from_slice(&audio_msg.sender_id.to_le_bytes());
                                    packet.extend_from_slice(&sender_addr_bytes);
                                    packet.extend_from_slice(&audio_msg.data);

                                    if dc_inner.send(&Bytes::from(packet)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        }
                    }
                });
            })
        }));

        let dc_msg = Arc::clone(&dc);
        dc.on_message(Box::new(move |msg: DataChannelMessage| {
            let dc_inner = Arc::clone(&dc_msg);
            Box::pin(async move {
                if msg.data.is_empty() {
                    return;
                }

                let msg_type = msg.data[0];
                let sender_addr = CLIENT_ADDRESSES.lock().unwrap().get(&client_id).cloned();

                if msg_type == 0x03 && msg.data.len() >= 33 {
                    // Targeted / Room audio message: [0x03, target_address (32 bytes SHA256), audio_data...]
                    let target_bytes: [u8; 32] = msg.data[1..33].try_into().unwrap();
                    let target_address = UserAddress::from_bytes(target_bytes);
                    let payload = msg.data[33..].to_vec();

                    if payload.len() > MAX_MESSAGE_SIZE {
                        warn!(
                            "Client {} sent oversized audio packet ({} bytes), ignoring",
                            client_id,
                            payload.len()
                        );
                        return;
                    }

                    // Enforce maximum 2 participants per call. Reject 3rd-party connection attempts.
                    if !is_client_in_same_call(client_id, &target_address)
                        && is_room_or_target_full(&target_address)
                    {
                        warn!(
                            "Rejecting Client {} connection to target {}: call already has maximum 2 participants",
                            client_id,
                            target_address.short_id()
                        );
                        let mut err_packet = Vec::with_capacity(33);
                        err_packet.push(0xFF);
                        err_packet.extend_from_slice(&target_address.to_bytes());
                        let _ = dc_inner.send(&Bytes::from(err_packet)).await;
                        return;
                    }

                    // Register/update client's target address for call routing
                    CLIENT_TARGETS
                        .lock()
                        .unwrap()
                        .insert(client_id, target_address.clone());

                    let _ = AUDIO_BROADCAST.send(AudioMessage {
                        sender_id: client_id,
                        sender_address: sender_addr,
                        target_address,
                        origin_node: my_node_id,
                        data: payload,
                    });
                }
            })
        }));

        Box::pin(async move {})
    }));

    peer_connection
        .set_remote_description(offer)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid offer SDP: {}", e)))?;

    let answer = peer_connection.create_answer(None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create answer: {}", e),
        )
    })?;

    peer_connection
        .set_local_description(answer)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set local description: {}", e),
            )
        })?;

    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    let _ = gather_complete.recv().await;

    let local_desc = peer_connection.local_description().await.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "No local description available".to_string(),
        )
    })?;

    Ok(Json(local_desc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_registered_addresses_empty() {
        let addrs = get_registered_addresses();
        assert!(
            addrs.is_empty() || !addrs.contains(&UserAddress::new("non_existent_test_id_12345"))
        );
    }

    #[test]
    fn test_client_addresses_registration() {
        let test_addr = UserAddress::generate_from_time();
        CLIENT_ADDRESSES
            .lock()
            .unwrap()
            .insert(999, test_addr.clone());

        let addrs = get_registered_addresses();
        assert!(addrs.contains(&test_addr));

        CLIENT_ADDRESSES.lock().unwrap().remove(&999);
        let addrs_after = get_registered_addresses();
        assert!(!addrs_after.contains(&test_addr));
    }

    #[test]
    fn test_participant_limit_2() {
        let host_addr = UserAddress::generate_from_time();
        let caller_addr = UserAddress::generate_from_time();
        let third_addr = UserAddress::generate_from_time();

        CLIENT_ADDRESSES
            .lock()
            .unwrap()
            .insert(101, host_addr.clone());
        CLIENT_ADDRESSES
            .lock()
            .unwrap()
            .insert(102, caller_addr.clone());
        CLIENT_TARGETS
            .lock()
            .unwrap()
            .insert(102, host_addr.clone());
        CLIENT_ADDRESSES
            .lock()
            .unwrap()
            .insert(103, third_addr.clone());

        assert_eq!(get_participant_count_for_room(&host_addr), 2);
        assert!(is_room_or_target_full(&host_addr));
        assert!(is_client_in_same_call(101, &host_addr));
        assert!(is_client_in_same_call(102, &host_addr));
        assert!(!is_client_in_same_call(103, &host_addr));

        // Cleanup
        CLIENT_ADDRESSES.lock().unwrap().remove(&101);
        CLIENT_ADDRESSES.lock().unwrap().remove(&102);
        CLIENT_ADDRESSES.lock().unwrap().remove(&103);
        CLIENT_TARGETS.lock().unwrap().remove(&102);
    }
}
