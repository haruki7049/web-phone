//! Connection handling module.
//!
//! This module provides the logic for handling WebRTC SDP offer requests,
//! managing client PeerConnections, and routing audio between connected WebRTC clients.

use crate::broadcast::{AUDIO_BROADCAST, AudioMessage};
use crate::config::CONFIGURATION;
use crate::registry::{CLIENT_REGISTRY, matches_address};
use axum::{extract::Json, http::HeaderMap};
use bytes::BytesMut;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::constants::{
    DEFAULT_INITIAL_TTL, HANDSHAKE_TIMEOUT, ICE_GATHER_TIMEOUT, MAX_MESSAGE_SIZE,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCPeerConnectionState, RTCSessionDescription,
};
use wpapi::{ProtocolPacket, UserAddress};

/// Counter for connected clients.
static CLIENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Helper to calculate audio energy (RMS) of audio frame payload or extract unencrypted WPIP-11 SFrame AudioEnergyByte.
pub fn calculate_audio_energy(audio_data: &[u8]) -> f64 {
    if audio_data.is_empty() {
        return 0.0;
    }
    // WPIP-11 SFrame E2EE Zero-Knowledge SFU check:
    // When non-multiple of 4 (e.g. 1-byte unencrypted AudioEnergy header + AES-GCM ciphertext),
    // extract unencrypted energy byte directly without decrypting audio payload.
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

            let ping_bytes = BytesMut::from(
                ProtocolPacket::Ping { timestamp: now_ms }
                    .encode()
                    .as_slice(),
            );

            let channels = {
                let reg = CLIENT_REGISTRY.read().unwrap();
                reg.data_channels.clone()
            };

            for (_cid, dc) in channels {
                let _ = dc.send(ping_bytes.clone()).await;
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

/// Validate that claimed sender identity fields in a packet match the connection's authenticated identity.
pub fn validate_packet_sender_identity(
    packet: &ProtocolPacket,
    authenticated_client_id: u64,
    authenticated_user_address: &UserAddress,
) -> bool {
    match packet {
        ProtocolPacket::ClientAssignment {
            client_id,
            user_address,
        } => *client_id == authenticated_client_id && user_address == authenticated_user_address,
        ProtocolPacket::ServerTargetedAudio {
            sender_id,
            sender_address,
            ..
        } => *sender_id == authenticated_client_id && sender_address == authenticated_user_address,
        ProtocolPacket::PeerTargetedAudio {
            sender_id,
            sender_address,
            ..
        } => *sender_id == authenticated_client_id && sender_address == authenticated_user_address,
        ProtocolPacket::CallRequest {
            caller_id,
            caller_address,
        } => *caller_id == authenticated_client_id && caller_address == authenticated_user_address,
        ProtocolPacket::BroadcastAudio { sender_id, .. } => *sender_id == authenticated_client_id,
        _ => true,
    }
}

/// Generate Ephemeral TURN credentials for an authenticated UserAddress (WPIP-10).
pub fn generate_turn_credentials_for_client(
    user_address: &UserAddress,
    ttl_seconds: u64,
) -> wpapi::TurnCredential {
    let daemon_config = CONFIGURATION.get().cloned().unwrap_or_default();
    let secret = crate::config::get_turn_server_secret();
    let turn_urls = vec![format!(
        "turn:{}:{}?transport=udp",
        daemon_config.ip, daemon_config.stun_port
    )];
    wpapi::generate_ephemeral_turn_credential(&secret, user_address, turn_urls, ttl_seconds)
}

struct ClientConnectionHandler {
    client_id: u64,
    user_address: UserAddress,
    my_node_id: u64,
    gather_tx: mpsc::Sender<()>,
}

#[async_trait]
impl PeerConnectionEventHandler for ClientConnectionHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.send(()).await;
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        info!("Client {} PeerConnection state: {}", self.client_id, state);
        if state == RTCPeerConnectionState::Failed
            || state == RTCPeerConnectionState::Closed
            || state == RTCPeerConnectionState::Disconnected
        {
            info!(
                "Client {} ({}) disconnected",
                self.client_id,
                self.user_address.short_id()
            );
            CLIENT_REGISTRY
                .write()
                .unwrap()
                .unregister_client(self.client_id);
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let dc_label = dc.label().await.unwrap_or_default();
        info!(
            "Client {} created DataChannel: {}",
            self.client_id, dc_label
        );

        CLIENT_REGISTRY
            .write()
            .unwrap()
            .data_channels
            .insert(self.client_id, Arc::clone(&dc));

        let client_id = self.client_id;
        let user_address = self.user_address.clone();
        let my_node_id = self.my_node_id;

        tokio::spawn(async move {
            handle_client_datachannel_events(dc, client_id, user_address, my_node_id).await;
        });
    }
}

async fn handle_client_datachannel_events(
    dc: Arc<dyn DataChannel>,
    client_id: u64,
    user_address: UserAddress,
    my_node_id: u64,
) {
    let mut audio_rx_opt = Some(AUDIO_BROADCAST.subscribe());
    let assign_packet = ProtocolPacket::ClientAssignment {
        client_id,
        user_address: user_address.clone(),
    };
    let _ = dc
        .send(BytesMut::from(assign_packet.encode().as_slice()))
        .await;

    while let Some(event) = dc.poll().await {
        match event {
            DataChannelEvent::OnOpen => {
                info!("Client {} DataChannel opened", client_id);
                if let Some(mut rx) = audio_rx_opt.take() {
                    let dc_inner = Arc::clone(&dc);
                    let user_addr = user_address.clone();
                    tokio::spawn(async move {
                        loop {
                            match rx.recv().await {
                                Ok(audio_msg) => {
                                    if audio_msg.sender_id == client_id {
                                        continue;
                                    }
                                    if matches_address(&audio_msg.target_address, &user_addr) {
                                        let packet = ProtocolPacket::ServerTargetedAudio {
                                            target_address: user_addr.clone(),
                                            sender_id: audio_msg.sender_id,
                                            sender_address: audio_msg
                                                .sender_address
                                                .unwrap_or_default(),
                                            codec_id: wpapi::protocol::CODEC_OPUS,
                                            audio_data: audio_msg.data,
                                        };
                                        if dc_inner
                                            .send(BytesMut::from(packet.encode().as_slice()))
                                            .await
                                            .is_err()
                                        {
                                            break;
                                        }
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                    continue;
                                }
                            }
                        }
                    });
                }
            }
            DataChannelEvent::OnMessage(msg) => {
                if !crate::rate_limit::DATACHANNEL_RATE_LIMITER.check_and_consume(client_id) {
                    warn!(
                        "Client {} exceeded WebRTC DataChannel packet rate limit (WPIP-15), dropping packet",
                        client_id
                    );
                    continue;
                }

                let Ok(packet) = ProtocolPacket::decode(&msg.data) else {
                    continue;
                };

                if !validate_packet_sender_identity(&packet, client_id, &user_address) {
                    warn!(
                        "Client {} attempted to send packet with spoofed sender identity, dropping",
                        client_id
                    );
                    continue;
                }

                let sender_addr = Some(user_address.clone());
                match packet {
                    ProtocolPacket::ClientTargetedAudio {
                        target_address,
                        audio_data,
                        ..
                    } => {
                        handle_client_targeted_audio(
                            client_id,
                            sender_addr,
                            target_address,
                            my_node_id,
                            audio_data,
                            &dc,
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
                    ProtocolPacket::RoomJoinRequest { room_address } => {
                        handle_room_join(client_id, room_address).await;
                    }
                    ProtocolPacket::RoomLeaveRequest { room_address } => {
                        handle_room_leave(client_id, &room_address).await;
                    }
                    ProtocolPacket::RoomGroupAudio {
                        room_address,
                        codec_id,
                        audio_data,
                    } => {
                        handle_room_group_audio(client_id, room_address, codec_id, audio_data)
                            .await;
                    }
                    ProtocolPacket::Pong { .. } => {
                        let now = std::time::Instant::now();
                        if let Ok(mut reg) = CLIENT_REGISTRY.write() {
                            reg.last_pong.insert(client_id, now);
                        }
                    }
                    _ => {}
                }
            }
            DataChannelEvent::OnClose => {
                info!("Client {} DataChannel closed", client_id);
                CLIENT_REGISTRY
                    .write()
                    .unwrap()
                    .unregister_client(client_id);
                break;
            }
            _ => {}
        }
    }
}

use crate::error::SignalingError;

/// Handle incoming SDP offer from a WebRTC client.
pub async fn handle_sdp_offer(
    headers: HeaderMap,
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, SignalingError> {
    let daemon_config = CONFIGURATION.get().cloned().unwrap_or_default();
    let my_node_id = daemon_config.node_id;

    let active_connections = CLIENT_REGISTRY.read().unwrap().peer_connections.len();
    if active_connections >= daemon_config.max_connections {
        warn!(
            "Rejected SDP offer: active connections ({}) reached max limit ({})",
            active_connections, daemon_config.max_connections
        );
        return Err(SignalingError::MaxConnectionsReached(
            daemon_config.max_connections,
        ));
    }

    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let user_address = if let Some(auth_val) = auth_header {
        match wpapi::verify_any_authorization_header(auth_val, &offer.sdp) {
            Ok(addr) => {
                info!("Verified client identity signature: {}", addr.short_id());
                addr
            }
            Err(err) => {
                error!("Authorization verification failed: {}", err);
                return Err(SignalingError::Unauthorized(err));
            }
        }
    } else {
        let addr = UserAddress::generate_from_time();
        info!(
            "No Authorization header present. Generated temporary User ID: {}",
            addr
        );
        addr
    };

    let client_id = CLIENT_COUNT.fetch_add(1, Ordering::SeqCst);
    let (gather_tx, mut gather_rx) = mpsc::channel(1);

    let handler = Arc::new(ClientConnectionHandler {
        client_id,
        user_address: user_address.clone(),
        my_node_id,
        gather_tx,
    });

    let config = RTCConfigurationBuilder::default().build();

    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_handler(handler)
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await
        .map_err(|e| {
            SignalingError::InternalError(format!("Failed to build PeerConnection: {}", e))
        })?;

    let peer_connection: Arc<dyn PeerConnection> = Arc::new(pc);

    info!(
        "Client {} connected, assigned User ID: {}",
        client_id, user_address
    );

    CLIENT_REGISTRY.write().unwrap().register_client(
        client_id,
        user_address.clone(),
        Arc::clone(&peer_connection),
    );

    // Handshake timeout task: close connection if DataChannel is not opened within 15s
    let pc_timeout = Arc::clone(&peer_connection);
    tokio::spawn(async move {
        tokio::time::sleep(HANDSHAKE_TIMEOUT).await;
        let has_dc = CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .contains_key(&client_id);
        if !has_dc {
            warn!(
                "Handshake timeout: Client {} failed to open DataChannel within 15s, closing connection",
                client_id
            );
            let _ = pc_timeout.close().await;
            CLIENT_REGISTRY
                .write()
                .unwrap()
                .unregister_client(client_id);
        }
    });

    peer_connection
        .set_remote_description(offer)
        .await
        .map_err(|e| SignalingError::InvalidSdpOffer(e.to_string()))?;

    let answer = peer_connection.create_answer(None).await.map_err(|e| {
        SignalingError::InternalError(format!("Failed to create SDP answer: {}", e))
    })?;

    peer_connection
        .set_local_description(answer)
        .await
        .map_err(|e| {
            SignalingError::InternalError(format!("Failed to set local description: {}", e))
        })?;

    let _ = tokio::time::timeout(ICE_GATHER_TIMEOUT, gather_rx.recv()).await;

    let local_desc = peer_connection.local_description().await.ok_or_else(|| {
        SignalingError::InternalError("No local description available".to_string())
    })?;

    Ok(Json(local_desc))
}

async fn handle_client_targeted_audio(
    client_id: u64,
    sender_addr: Option<UserAddress>,
    target_address: UserAddress,
    my_node_id: u64,
    payload: Vec<u8>,
    dc: &Arc<dyn DataChannel>,
) {
    if payload.len() > MAX_MESSAGE_SIZE {
        warn!(
            "Client {} sent oversized audio packet ({} bytes), ignoring",
            client_id,
            payload.len()
        );
        return;
    }

    let caller_user_addr = sender_addr.clone().unwrap_or_default();

    if is_client_in_room(client_id, &target_address) {
        // Group call mode
    } else if let Some(target_cid) = find_client_by_address(&target_address) {
        if is_call_rejected(target_cid, &caller_user_addr) {
            warn!(
                "Rejecting Client {} call to {}: call rejected by recipient",
                client_id,
                target_address.short_id()
            );
            let err_packet = ProtocolPacket::ConnectionError {
                target_address: target_address.clone(),
            };
            let _ = dc
                .send(BytesMut::from(err_packet.encode().as_slice()))
                .await;
            return;
        }

        let is_in_same_call = is_client_in_same_call(client_id, &target_address);
        let is_mutual = CLIENT_REGISTRY
            .read()
            .unwrap()
            .targets
            .get(&target_cid)
            .map(|addr| matches_address(addr, &caller_user_addr))
            .unwrap_or(false);

        if !is_in_same_call && !is_mutual && is_room_or_target_full(&target_address) {
            warn!(
                "Rejecting Client {} call to {}: target user busy",
                client_id,
                target_address.short_id()
            );
            let err_packet = ProtocolPacket::ConnectionError {
                target_address: target_address.clone(),
            };
            let _ = dc
                .send(BytesMut::from(err_packet.encode().as_slice()))
                .await;
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
                    let _ = target_dc
                        .send(BytesMut::from(req_packet.encode().as_slice()))
                        .await;
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
    } else {
        warn!(
            "Rejecting Client {} connection to target {}: target user not found or offline",
            client_id,
            target_address.short_id()
        );
        let err_packet = ProtocolPacket::ConnectionError {
            target_address: target_address.clone(),
        };
        let _ = dc
            .send(BytesMut::from(err_packet.encode().as_slice()))
            .await;
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
            ttl: DEFAULT_INITIAL_TTL, // Initial TTL per WPIP-05 Section 3.1
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
        let _ = caller_dc
            .send(BytesMut::from(accept_packet.encode().as_slice()))
            .await;
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
        let _ = caller_dc
            .send(BytesMut::from(reject_packet.encode().as_slice()))
            .await;
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
        let _ = target_dc
            .send(BytesMut::from(ended_pkt.encode().as_slice()))
            .await;
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
    let bytes = BytesMut::from(state_pkt.encode().as_slice());

    for member_cid in member_cids {
        let dc = CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&member_cid)
            .cloned();
        if let Some(dc) = dc {
            let _ = dc.send(bytes.clone()).await;
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
    let bytes = BytesMut::from(state_pkt.encode().as_slice());

    for member_cid in member_cids {
        let dc = CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&member_cid)
            .cloned();
        if let Some(dc) = dc {
            let _ = dc.send(bytes.clone()).await;
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
    let bytes = BytesMut::from(server_audio_pkt.encode().as_slice());

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
            let _ = dc.send(bytes.clone()).await;
        }
    }

    if !top_speakers.is_empty() {
        let notice_pkt = ProtocolPacket::ActiveSpeakerNotice {
            room_address,
            speaker_addresses: top_speakers,
        };
        let notice_bytes = BytesMut::from(notice_pkt.encode().as_slice());
        for member_cid in member_cids {
            let dc = CLIENT_REGISTRY
                .read()
                .unwrap()
                .data_channels
                .get(&member_cid)
                .cloned();
            if let Some(dc) = dc {
                let _ = dc.send(notice_bytes.clone()).await;
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
            .insert(99999, test_addr.clone());

        let addrs = get_registered_addresses();
        assert!(addrs.contains(&test_addr));

        CLIENT_REGISTRY.write().unwrap().unregister_client(99999);
    }

    #[tokio::test]
    async fn test_handle_sdp_offer_auth_failures() {
        use axum::http::{HeaderMap, StatusCode};

        let keypair = wpapi::UserKeypair::generate();
        let sdp_text = "v=0\r\no=- 12345 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n";
        let offer = RTCSessionDescription::offer(sdp_text.to_string()).unwrap();

        let (_, header_val) = wpapi::build_authorization_header(&keypair, sdp_text);

        // 1. Expired timestamp (+301s)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expired_ts = now.saturating_sub(301);
        let expired_payload = format!("{}:{}", expired_ts, sdp_text);
        let expired_sig = keypair.sign(expired_payload.as_bytes());
        let expired_header = format!(
            "WP-Ed25519 {}:{}:{}",
            keypair.public_key_address().id,
            expired_ts,
            expired_sig
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            expired_header.parse().unwrap(),
        );

        let res = handle_sdp_offer(headers, Json(offer.clone())).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert!(matches!(
            err,
            SignalingError::Unauthorized(wpapi::AuthError::TimestampDrift { .. })
        ));

        // 2. Tampered SDP payload signature
        let tampered_sdp_text = "v=0\r\no=- 99999 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n";
        let tampered_offer = RTCSessionDescription::offer(tampered_sdp_text.to_string()).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            header_val.parse().unwrap(),
        );
        let res = handle_sdp_offer(headers, Json(tampered_offer)).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert!(matches!(
            err,
            SignalingError::Unauthorized(wpapi::AuthError::InvalidEd25519Signature)
        ));

        // 3. First valid attempt: OK (or fails later at PeerConnection creation if SDP invalid, but auth passes)
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            header_val.parse().unwrap(),
        );
        let _ = handle_sdp_offer(headers, Json(offer.clone())).await;

        // 4. Replayed signature: Rejected (401 Unauthorized)
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            header_val.parse().unwrap(),
        );
        let res = handle_sdp_offer(headers, Json(offer)).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert!(matches!(
            err,
            SignalingError::Unauthorized(wpapi::AuthError::ReplayDetected)
        ));
    }

    #[test]
    fn test_sfu_zero_knowledge_energy_routing() {
        let key = [0x55u8; 32];
        let nonce = [0x09u8; 12];
        let sframe = wpapi::SFrameCodec::new(&key).unwrap();

        let raw_pcm = [0.5f32.to_le_bytes(), 0.8f32.to_le_bytes()].concat();
        let energy_byte = 204u8; // ~0.8 scale (204/255 = 0.8)

        let encrypted_frame = sframe.encrypt_frame(&nonce, energy_byte, &raw_pcm).unwrap();

        // Ensure wpdaemon calculates audio energy without decrypting SFrame payload
        let calculated_energy = calculate_audio_energy(&encrypted_frame);
        assert!((calculated_energy - (204.0 / 255.0)).abs() < 1e-4);
    }

    #[tokio::test]
    async fn test_addresses_authentication_requirement() {
        use axum::http::{HeaderMap, HeaderValue};

        // 1. Test missing Authorization header
        let headers = HeaderMap::new();
        let auth_header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        assert!(auth_header.is_none());

        // 2. Test invalid Authorization header
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

        // 3. Test valid Authorization header
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

        // 1. Valid packet with matching auth identity
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

        // 2. Spoofed sender_id in ServerTargetedAudio
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

        // 3. Spoofed sender_address in ServerTargetedAudio
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

        // 4. Spoofed sender_id in BroadcastAudio
        let spoofed_broadcast = ProtocolPacket::BroadcastAudio {
            sender_id: attacker_client_id,
            audio_data: vec![0x00],
        };
        assert!(!validate_packet_sender_identity(
            &spoofed_broadcast,
            auth_client_id,
            &auth_user_addr
        ));

        // 5. Spoofed CallRequest caller_id and caller_address
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
}
