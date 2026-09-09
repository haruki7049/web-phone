//! Connection handling module.
//!
//! This module provides the logic for handling WebRTC SDP offer requests,
//! managing client PeerConnections, and routing audio between connected WebRTC clients.

use crate::broadcast::{AUDIO_BROADCAST, AudioMessage};
use crate::config::CONFIGURATION;
use crate::registry::{CLIENT_REGISTRY, matches_address};
use axum::{
    extract::Json,
    http::{HeaderMap, StatusCode},
};
use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};

use std::sync::Arc;
use tracing::{error, info, warn};
use webrtc::api::APIBuilder;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use wpapi::{ProtocolPacket, UserAddress};

/// Counter for connected clients.
static CLIENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Maximum audio message size in bytes (1MB).
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Helper to calculate audio energy (RMS) of audio frame payload.
pub fn calculate_audio_energy(audio_data: &[u8]) -> f64 {
    if audio_data.is_empty() {
        return 0.0;
    }
    if audio_data.len() >= 4 && audio_data.len().is_multiple_of(4) {
        let (chunks, _) = audio_data.as_chunks::<4>();
        let sum_sq: f64 = chunks
            .iter()
            .map(|c| {
                let sample = f32::from_le_bytes(*c) as f64;
                sample * sample
            })
            .sum();
        (sum_sq / chunks.len() as f64).sqrt()
    } else {
        let sum_sq: f64 = audio_data
            .iter()
            .map(|&b| {
                let val = (b as f64) - 128.0;
                val * val
            })
            .sum();
        (sum_sq / audio_data.len() as f64).sqrt()
    }
}

/// Start background keep-alive heartbeat task (WPIP-09).
pub fn start_keepalive_task() {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let ping_bytes = Bytes::from(ProtocolPacket::Ping { timestamp: now_ms }.encode());

            let channels = {
                let reg = CLIENT_REGISTRY.read().unwrap();
                reg.data_channels.clone()
            };

            for (_cid, dc) in channels {
                let _ = dc.send(&ping_bytes).await;
            }

            let stale_cids = {
                let reg = CLIENT_REGISTRY.read().unwrap();
                reg.get_stale_clients(30)
            };

            for cid in stale_cids {
                warn!(
                    "Client {} failed keep-alive pong response for >30s, terminating connection",
                    cid
                );
                let pc = {
                    let reg = CLIENT_REGISTRY.read().unwrap();
                    reg.peer_connections.get(&cid).cloned()
                };
                if let Some(pc) = pc {
                    let _ = pc.close().await;
                }
                CLIENT_REGISTRY.write().unwrap().unregister_client(cid);
            }
        }
    });
}

/// Find client ID matching a given UserAddress (exact or prefix match).
fn find_client_by_address(target_key: &UserAddress) -> Option<u64> {
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

/// Check if a client belongs to the ad-hoc room identified by room_key.
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

/// Check if client_id is already part of the call with target_key.
fn is_client_in_same_call(client_id: u64, target_key: &UserAddress) -> bool {
    CLIENT_REGISTRY
        .read()
        .unwrap()
        .is_client_in_same_call(client_id, target_key)
}

/// Check if room or target user is already in a call with maximum allowed participants (2).
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

/// Handle an SDP offer from a WebRTC client.
pub async fn handle_sdp_offer(
    headers: HeaderMap,
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, (StatusCode, String)> {
    let daemon_config = CONFIGURATION.get().cloned().unwrap_or_default();
    let my_node_id = daemon_config.node_id;

    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let user_address = if let Some(auth_val) = auth_header {
        match wpapi::address::verify_authorization_header(auth_val, &offer.sdp) {
            Ok(addr) => {
                info!("Verified client Ed25519 identity signature: {}", addr);
                addr
            }
            Err(err_msg) => {
                error!("Ed25519 authorization verification failed: {}", err_msg);
                return Err((
                    StatusCode::UNAUTHORIZED,
                    format!("401 Unauthorized: {}", err_msg),
                ));
            }
        }
    } else {
        info!("No Authorization header provided, fallback to temporary UserAddress...");
        UserAddress::generate_from_time()
    };

    let api = APIBuilder::new().build();
    let config = RTCConfiguration::default();

    let peer_connection = Arc::new(api.new_peer_connection(config).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create PeerConnection: {}", e),
        )
    })?);

    let client_id = CLIENT_COUNT.fetch_add(1, Ordering::SeqCst);

    info!(
        "Client {} connected, assigned User ID: {}",
        client_id, user_address
    );

    CLIENT_REGISTRY.write().unwrap().register_client(
        client_id,
        user_address.clone(),
        Arc::clone(&peer_connection),
    );

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
                CLIENT_REGISTRY
                    .write()
                    .unwrap()
                    .unregister_client(client_id);
            }
            Box::pin(async move {})
        },
    ));

    peer_connection.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        let dc_label = dc.label().to_string();
        info!("Client {} created DataChannel: {}", client_id, dc_label);

        CLIENT_REGISTRY
            .write()
            .unwrap()
            .data_channels
            .insert(client_id, Arc::clone(&dc));

        let dc_open = Arc::clone(&dc);

        dc.on_open(Box::new(move || {
            let dc_inner = Arc::clone(&dc_open);
            let mut audio_rx = AUDIO_BROADCAST.subscribe();

            Box::pin(async move {
                info!("Client {} DataChannel opened", client_id);

                let my_addr = CLIENT_REGISTRY
                    .read()
                    .unwrap()
                    .addresses
                    .get(&client_id)
                    .cloned()
                    .unwrap_or_default();

                // Send client ID and registered SHA-256 address assignment message:
                let init_packet = ProtocolPacket::ClientAssignment {
                    client_id,
                    user_address: my_addr,
                };

                if let Err(e) = dc_inner.send(&Bytes::from(init_packet.encode())).await {
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
                                    let packet = ProtocolPacket::ServerTargetedAudio {
                                        target_address: target_room.clone(),
                                        sender_id: audio_msg.sender_id,
                                        sender_address: audio_msg
                                            .sender_address
                                            .unwrap_or_default(),
                                        codec_id: wpapi::protocol::CODEC_OPUS,
                                        audio_data: audio_msg.data,
                                    };

                                    if dc_inner.send(&Bytes::from(packet.encode())).await.is_err() {
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
                if let Ok(packet) = ProtocolPacket::decode(&msg.data) {
                    process_incoming_packet(client_id, my_node_id, &dc_inner, packet).await;
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

/// Process incoming DataChannel packet according to packet type.
async fn process_incoming_packet(
    client_id: u64,
    my_node_id: u64,
    dc: &Arc<RTCDataChannel>,
    packet: ProtocolPacket,
) {
    let sender_addr = CLIENT_REGISTRY
        .read()
        .unwrap()
        .addresses
        .get(&client_id)
        .cloned();

    match packet {
        ProtocolPacket::ClientTargetedAudio {
            target_address,
            audio_data: payload,
            ..
        } => {
            handle_client_targeted_audio(
                client_id,
                my_node_id,
                dc,
                sender_addr,
                target_address,
                payload,
            )
            .await;
        }
        ProtocolPacket::CallAcceptResponse { caller_address } => {
            handle_call_accept_response(client_id, caller_address).await;
        }
        ProtocolPacket::CallRejectResponse { caller_address } => {
            handle_call_reject_response(client_id, caller_address).await;
        }
        ProtocolPacket::CallHangup { target_address } => {
            handle_call_hangup(client_id, sender_addr, target_address).await;
        }
        ProtocolPacket::Ping { timestamp } => {
            let pong = ProtocolPacket::Pong { timestamp };
            let _ = dc.send(&Bytes::from(pong.encode())).await;
        }
        ProtocolPacket::Pong { .. } => {
            CLIENT_REGISTRY.write().unwrap().update_last_pong(client_id);
        }
        ProtocolPacket::RoomJoinRequest { room_address } => {
            handle_room_join(client_id, room_address).await;
        }
        ProtocolPacket::RoomLeaveRequest { room_address } => {
            handle_room_leave(client_id, &room_address).await;
        }
        ProtocolPacket::RoomGroupAudio {
            room_address,
            codec_id,
            audio_data: payload,
        } => {
            handle_room_group_audio(client_id, room_address, codec_id, payload).await;
        }
        _ => {}
    }
}

async fn handle_client_targeted_audio(
    client_id: u64,
    my_node_id: u64,
    dc: &Arc<RTCDataChannel>,
    sender_addr: Option<UserAddress>,
    target_address: UserAddress,
    payload: Vec<u8>,
) {
    if payload.len() > MAX_MESSAGE_SIZE {
        warn!(
            "Client {} sent oversized audio packet ({} bytes), ignoring",
            client_id,
            payload.len()
        );
        return;
    }

    if !is_client_in_same_call(client_id, &target_address)
        && is_room_or_target_full(&target_address)
    {
        warn!(
            "Rejecting Client {} connection to target {}: call already has maximum 2 participants",
            client_id,
            target_address.short_id()
        );
        let err_packet = ProtocolPacket::ConnectionError {
            target_address: target_address.clone(),
        };
        let _ = dc.send(&Bytes::from(err_packet.encode())).await;
        return;
    }

    if let Some(target_cid) = find_client_by_address(&target_address) {
        if target_cid != client_id {
            let caller_user_addr = sender_addr.clone().unwrap_or_default();

            let target_explicit_target = CLIENT_REGISTRY
                .read()
                .unwrap()
                .targets
                .get(&target_cid)
                .cloned();
            let is_mutual = target_explicit_target
                .as_ref()
                .is_some_and(|t| matches_address(t, &caller_user_addr));

            if is_call_rejected(target_cid, &caller_user_addr) {
                let err_packet = ProtocolPacket::CallRejectedNotification {
                    target_address: target_address.clone(),
                };
                let _ = dc.send(&Bytes::from(err_packet.encode())).await;
                return;
            }

            if !is_mutual && !is_call_approved(target_cid, &caller_user_addr) {
                if !has_been_notified(target_cid, client_id) {
                    mark_notified(target_cid, client_id);
                    let target_dc = CLIENT_REGISTRY
                        .read()
                        .unwrap()
                        .data_channels
                        .get(&target_cid)
                        .cloned();
                    if let Some(target_dc) = target_dc {
                        let req_packet = ProtocolPacket::CallRequest {
                            caller_id: client_id,
                            caller_address: caller_user_addr.clone(),
                        };
                        let _ = target_dc.send(&Bytes::from(req_packet.encode())).await;
                        info!(
                            "Sent call request notification to Client {} for caller Client {} ({})",
                            target_cid,
                            client_id,
                            caller_user_addr.short_id()
                        );
                    }
                }
                return;
            }
        }
    } else {
        warn!(
            "Rejecting Client {} connection to target {}: target user not found or offline",
            client_id,
            target_address.short_id()
        );
        let err_packet = ProtocolPacket::ConnectionError {
            target_address: target_address.clone(),
        };
        let _ = dc.send(&Bytes::from(err_packet.encode())).await;
        return;
    }

    CLIENT_REGISTRY
        .write()
        .unwrap()
        .targets
        .insert(client_id, target_address.clone());

    if !payload.is_empty() {
        let _ = AUDIO_BROADCAST.send(AudioMessage {
            sender_id: client_id,
            sender_address: sender_addr,
            target_address,
            origin_node: my_node_id,
            data: payload,
        });
    }
}

async fn handle_call_accept_response(client_id: u64, caller_address: UserAddress) {
    info!(
        "Client {} accepted call request from caller {}",
        client_id,
        caller_address.short_id()
    );
    mark_call_approved(client_id, caller_address.clone());

    {
        let mut reg = CLIENT_REGISTRY.write().unwrap();
        reg.targets
            .entry(client_id)
            .or_insert_with(|| caller_address.clone());
    }

    let caller_dc = find_client_by_address(&caller_address).and_then(|caller_cid| {
        CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&caller_cid)
            .cloned()
    });

    if let Some(caller_dc) = caller_dc {
        let my_addr = CLIENT_REGISTRY
            .read()
            .unwrap()
            .addresses
            .get(&client_id)
            .cloned()
            .unwrap_or_default();
        let accept_packet = ProtocolPacket::CallAcceptedNotification {
            target_address: my_addr,
        };
        let _ = caller_dc.send(&Bytes::from(accept_packet.encode())).await;
    }
}

async fn handle_call_reject_response(client_id: u64, caller_address: UserAddress) {
    info!(
        "Client {} rejected call request from caller {}",
        client_id,
        caller_address.short_id()
    );
    mark_call_rejected(client_id, caller_address.clone());

    let caller_dc = find_client_by_address(&caller_address).and_then(|caller_cid| {
        CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&caller_cid)
            .cloned()
    });

    if let Some(caller_dc) = caller_dc {
        let my_addr = CLIENT_REGISTRY
            .read()
            .unwrap()
            .addresses
            .get(&client_id)
            .cloned()
            .unwrap_or_default();
        let reject_packet = ProtocolPacket::CallRejectedNotification {
            target_address: my_addr,
        };
        let _ = caller_dc.send(&Bytes::from(reject_packet.encode())).await;
    }
}

async fn handle_call_hangup(
    client_id: u64,
    sender_addr: Option<UserAddress>,
    target_address: UserAddress,
) {
    let my_addr = sender_addr.unwrap_or_default();
    info!(
        "Client {} ({}) initiated CallHangup for target {}",
        client_id,
        my_addr.short_id(),
        target_address.short_id()
    );
    CLIENT_REGISTRY
        .write()
        .unwrap()
        .clear_call_session(client_id, &target_address);

    let target_dc = find_client_by_address(&target_address).and_then(|target_cid| {
        CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&target_cid)
            .cloned()
    });

    if let Some(target_dc) = target_dc {
        let ended_pkt = ProtocolPacket::CallEndedNotification {
            target_address: my_addr,
        };
        let _ = target_dc.send(&Bytes::from(ended_pkt.encode())).await;
    }
}

async fn handle_room_join(client_id: u64, room_address: UserAddress) {
    let (member_count, member_cids) = {
        let mut reg = CLIENT_REGISTRY.write().unwrap();
        let count = reg.join_room(client_id, room_address.clone());
        let members = reg.get_room_member_ids(&room_address);
        (count, members)
    };

    info!(
        "Client {} joined room {} (Total participants: {})",
        client_id,
        room_address.short_id(),
        member_count
    );

    let state_pkt = ProtocolPacket::RoomStateNotification {
        room_address,
        participant_count: member_count,
    };
    let bytes = Bytes::from(state_pkt.encode());

    for member_cid in member_cids {
        let dc = CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&member_cid)
            .cloned();
        if let Some(dc) = dc {
            let _ = dc.send(&bytes).await;
        }
    }
}

async fn handle_room_leave(client_id: u64, room_address: &UserAddress) {
    let (remaining_count, member_cids) = {
        let mut reg = CLIENT_REGISTRY.write().unwrap();
        let count = reg.leave_room(client_id, room_address);
        let members = reg.get_room_member_ids(room_address);
        (count, members)
    };

    info!(
        "Client {} left room {} (Remaining participants: {})",
        client_id,
        room_address.short_id(),
        remaining_count
    );

    let state_pkt = ProtocolPacket::RoomStateNotification {
        room_address: room_address.clone(),
        participant_count: remaining_count,
    };
    let bytes = Bytes::from(state_pkt.encode());

    for member_cid in member_cids {
        let dc = CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&member_cid)
            .cloned();
        if let Some(dc) = dc {
            let _ = dc.send(&bytes).await;
        }
    }
}

async fn handle_room_group_audio(
    client_id: u64,
    room_address: UserAddress,
    codec_id: u8,
    payload: Vec<u8>,
) {
    if payload.len() > MAX_MESSAGE_SIZE {
        warn!(
            "Client {} sent oversized group audio packet ({} bytes), ignoring",
            client_id,
            payload.len()
        );
        return;
    }

    let energy = calculate_audio_energy(&payload);
    let (is_top_k, top_speakers, _) = {
        let mut reg = CLIENT_REGISTRY.write().unwrap();
        reg.update_speaker_energy(&room_address, client_id, energy)
    };

    if !is_top_k {
        return;
    }

    let member_cids = CLIENT_REGISTRY
        .read()
        .unwrap()
        .get_room_member_ids(&room_address);

    let server_audio_pkt = ProtocolPacket::RoomGroupAudio {
        room_address: room_address.clone(),
        codec_id,
        audio_data: payload,
    };
    let bytes = Bytes::from(server_audio_pkt.encode());

    for member_cid in &member_cids {
        if *member_cid == client_id {
            continue;
        }
        let dc = CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(member_cid)
            .cloned();
        if let Some(dc) = dc {
            let _ = dc.send(&bytes).await;
        }
    }

    if !top_speakers.is_empty() {
        let notice_pkt = ProtocolPacket::ActiveSpeakerNotice {
            room_address,
            speaker_addresses: top_speakers,
        };
        let notice_bytes = Bytes::from(notice_pkt.encode());
        for member_cid in member_cids {
            let dc = CLIENT_REGISTRY
                .read()
                .unwrap()
                .data_channels
                .get(&member_cid)
                .cloned();
            if let Some(dc) = dc {
                let _ = dc.send(&notice_bytes).await;
            }
        }
    }
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
        CLIENT_REGISTRY
            .write()
            .unwrap()
            .addresses
            .insert(999, test_addr.clone());

        let addrs = get_registered_addresses();
        assert!(addrs.contains(&test_addr));

        CLIENT_REGISTRY.write().unwrap().addresses.remove(&999);
        let addrs_after = get_registered_addresses();
        assert!(!addrs_after.contains(&test_addr));
    }

    #[test]
    fn test_participant_limit_2() {
        let host_addr = UserAddress::generate_from_time();
        let caller_addr = UserAddress::generate_from_time();
        let third_addr = UserAddress::generate_from_time();

        {
            let mut reg = CLIENT_REGISTRY.write().unwrap();
            reg.addresses.insert(101, host_addr.clone());
            reg.addresses.insert(102, caller_addr.clone());
            reg.targets.insert(102, host_addr.clone());
            reg.addresses.insert(103, third_addr.clone());
        }

        assert_eq!(get_participant_count_for_room(&host_addr), 2);
        assert!(is_room_or_target_full(&host_addr));
        assert!(is_client_in_same_call(101, &host_addr));
        assert!(is_client_in_same_call(102, &host_addr));
        assert!(!is_client_in_same_call(103, &host_addr));

        // Cleanup
        {
            let mut reg = CLIENT_REGISTRY.write().unwrap();
            reg.addresses.remove(&101);
            reg.addresses.remove(&102);
            reg.addresses.remove(&103);
            reg.targets.remove(&102);
        }
    }

    #[test]
    fn test_call_approval_and_rejection_states() {
        let host_id = 201u64;
        let caller_addr = UserAddress::generate_from_time();

        assert!(!is_call_approved(host_id, &caller_addr));
        assert!(!is_call_rejected(host_id, &caller_addr));

        mark_call_approved(host_id, caller_addr.clone());
        assert!(is_call_approved(host_id, &caller_addr));

        mark_call_rejected(host_id, caller_addr.clone());
        assert!(is_call_rejected(host_id, &caller_addr));

        // Cleanup
        {
            let mut reg = CLIENT_REGISTRY.write().unwrap();
            reg.approved_calls.remove(&host_id);
            reg.rejected_calls.remove(&host_id);
        }
    }

    #[test]
    fn test_notification_tracking() {
        let target_id = 301u64;
        let caller_id = 302u64;

        assert!(!has_been_notified(target_id, caller_id));

        mark_notified(target_id, caller_id);
        assert!(has_been_notified(target_id, caller_id));

        // Cleanup
        CLIENT_REGISTRY
            .write()
            .unwrap()
            .notified_requests
            .remove(&target_id);
    }

    #[test]
    fn test_matches_address() {
        let full_addr = UserAddress::new("a1b2c3d4e5f607080900112233445566");
        let short_key = UserAddress::new("a1b2c3d4e5f6");
        let zero_padded_short =
            UserAddress::new("a1b2c3d4e5f60000000000000000000000000000000000000000000000000000");
        let different = UserAddress::new("fffffffff");

        assert!(matches_address(&full_addr, &full_addr));
        assert!(matches_address(&full_addr, &short_key));
        assert!(matches_address(&short_key, &full_addr));
        assert!(matches_address(&full_addr, &zero_padded_short));
        assert!(!matches_address(&full_addr, &different));
    }

    #[test]
    fn test_find_client_by_address() {
        let client_id = 888u64;
        let addr = UserAddress::new("11223344556677889900aabbccddeeff");
        CLIENT_REGISTRY
            .write()
            .unwrap()
            .addresses
            .insert(client_id, addr.clone());

        let short_search = UserAddress::new("112233445566");
        assert_eq!(find_client_by_address(&addr), Some(client_id));
        assert_eq!(find_client_by_address(&short_search), Some(client_id));

        let not_found = UserAddress::new("999999");
        assert_eq!(find_client_by_address(&not_found), None);

        // Cleanup
        CLIENT_REGISTRY
            .write()
            .unwrap()
            .addresses
            .remove(&client_id);
    }

    #[test]
    fn test_clear_call_session() {
        let client_a = 501u64;
        let client_b = 502u64;
        let addr_a = UserAddress::generate_from_time();
        let addr_b = UserAddress::generate_from_time();

        {
            let mut reg = CLIENT_REGISTRY.write().unwrap();
            reg.addresses.insert(client_a, addr_a.clone());
            reg.addresses.insert(client_b, addr_b.clone());
            reg.approved_calls
                .entry(client_a)
                .or_default()
                .push(addr_b.clone());
            reg.approved_calls
                .entry(client_b)
                .or_default()
                .push(addr_a.clone());
            reg.notified_requests
                .entry(client_b)
                .or_default()
                .push(client_a);
        }

        assert!(is_call_approved(client_a, &addr_b));

        // Perform clear_call_session for CallHangup
        CLIENT_REGISTRY
            .write()
            .unwrap()
            .clear_call_session(client_a, &addr_b);

        assert!(!is_call_approved(client_a, &addr_b));
        assert!(!is_call_approved(client_b, &addr_a));

        // Cleanup
        {
            let mut reg = CLIENT_REGISTRY.write().unwrap();
            reg.addresses.remove(&client_a);
            reg.addresses.remove(&client_b);
            reg.approved_calls.remove(&client_a);
            reg.approved_calls.remove(&client_b);
            reg.notified_requests.remove(&client_b);
        }
    }

    #[test]
    fn test_short_id_minimum_length_rule() {
        let full = UserAddress::new("1234567890abcdef1234567890abcdef");
        assert!(full.id.len() >= 12);

        let short_valid = UserAddress::new("1234567890ab");
        assert_eq!(short_valid.id.len(), 12);
        assert!(short_valid.id.len() >= 12);

        let short_invalid = UserAddress::new("1234567890a");
        assert!(short_invalid.id.len() < 12);
    }
}
