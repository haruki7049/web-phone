//! WebRTC DataChannel event loop and packet routing handlers.

use crate::broadcast::{AUDIO_BROADCAST, AudioMessage};
use crate::constants::{DEFAULT_INITIAL_TTL, MAX_MESSAGE_SIZE};
use crate::registry::{CLIENT_REGISTRY, matches_address_prefix};
use bytes::BytesMut;
use std::sync::Arc;
use tracing::{info, warn};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use wpapi::{ProtocolPacket, UserAddress};

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

pub(crate) async fn handle_client_datachannel_events(
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
                                    if matches_address_prefix(&user_addr, &audio_msg.target_address)
                                    {
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

                dispatch_daemon_packet(client_id, &user_address, my_node_id, packet, &dc).await;
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

async fn dispatch_daemon_packet(
    client_id: u64,
    user_address: &UserAddress,
    my_node_id: u64,
    packet: ProtocolPacket,
    dc: &Arc<dyn DataChannel>,
) {
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
                dc,
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
            handle_room_group_audio(client_id, room_address, codec_id, audio_data).await;
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

    if super::is_client_in_room(client_id, &target_address) {
        // Group call mode
    } else if let crate::registry::AddressSearchResult::Found(target_cid) =
        super::find_client_by_address(&target_address)
    {
        let is_stale = {
            let reg = CLIENT_REGISTRY.read().unwrap();
            reg.get_stale_clients(15).contains(&target_cid)
        };
        if is_stale {
            warn!(
                "Rejecting Client {} call to {}: target peer connection is stale/unresponsive (waiting for keep-alive cleanup)",
                client_id,
                target_address.short_id()
            );
            CLIENT_REGISTRY
                .write()
                .unwrap()
                .unregister_client(target_cid);
            let err_packet = ProtocolPacket::ConnectionError {
                target_address: target_address.clone(),
            };
            let _ = dc
                .send(BytesMut::from(err_packet.encode().as_slice()))
                .await;
            return;
        }

        if target_cid == client_id {
            warn!(
                "Rejecting Client {} call to {}: target is self",
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

        if super::is_call_rejected(target_cid, &caller_user_addr) {
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

        let is_in_same_call = super::is_client_in_same_call(client_id, &target_address);
        let is_mutual = CLIENT_REGISTRY
            .read()
            .unwrap()
            .targets
            .get(&target_cid)
            .map(|addr| matches_address_prefix(addr, &caller_user_addr))
            .unwrap_or(false);

        if !is_in_same_call && !is_mutual && super::is_room_or_target_full(&target_address) {
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

        if !is_mutual && !super::is_call_approved(target_cid, &caller_user_addr) {
            if !super::has_been_notified(target_cid, client_id) {
                super::mark_notified(target_cid, client_id);
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
            ttl: DEFAULT_INITIAL_TTL,
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
    super::mark_call_approved(client_id, caller_address.clone());

    {
        let mut reg = CLIENT_REGISTRY.write().unwrap();
        reg.targets
            .entry(client_id)
            .or_insert_with(|| caller_address.clone());
    }

    let caller_dc = if let crate::registry::AddressSearchResult::Found(caller_cid) =
        super::find_client_by_address(&caller_address)
    {
        CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&caller_cid)
            .cloned()
    } else {
        None
    };

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
    super::mark_call_rejected(client_id, caller_address.clone());

    let caller_dc = if let crate::registry::AddressSearchResult::Found(caller_cid) =
        super::find_client_by_address(&caller_address)
    {
        CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&caller_cid)
            .cloned()
    } else {
        None
    };

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

    let target_dc = if let crate::registry::AddressSearchResult::Found(target_cid) =
        super::find_client_by_address(&target_address)
    {
        CLIENT_REGISTRY
            .read()
            .unwrap()
            .data_channels
            .get(&target_cid)
            .cloned()
    } else {
        None
    };

    if let Some(target_dc) = target_dc {
        let ended_pkt = ProtocolPacket::CallEndedNotification {
            target_address: my_addr.clone(),
        };
        info!(
            "Sent CallEndedNotification to target {} for call ended by Client {} ({})",
            target_address.short_id(),
            client_id,
            my_addr.short_id()
        );
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

    let energy = super::calculate_audio_energy(&payload);
    let (is_top_k, top_speakers, speaker_list_changed) = {
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

    if speaker_list_changed && !top_speakers.is_empty() {
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
