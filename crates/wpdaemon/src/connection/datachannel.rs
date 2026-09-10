//! WebRTC DataChannel event loop and packet routing handlers.

use crate::broadcast::{AUDIO_BROADCAST, AudioMessage};
use crate::registry::{CLIENT_REGISTRY, matches_address};
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

async fn handle_client_targeted_audio(
    client_id: u64,
    sender_addr: Option<UserAddress>,
    target_address: UserAddress,
    my_node_id: u64,
    audio_data: Vec<u8>,
    sender_dc: &Arc<dyn DataChannel>,
) {
    if let Some(target_cid) = super::find_client_by_address(&target_address) {
        if super::is_call_rejected(target_cid, sender_addr.as_ref().unwrap()) {
            info!(
                "Call to client {} was rejected, dropping audio and notifying caller",
                target_cid
            );
            let notif = ProtocolPacket::CallRejectedNotification {
                target_address: target_address.clone(),
            };
            let _ = sender_dc
                .send(BytesMut::from(notif.encode().as_slice()))
                .await;
            return;
        }

        if !super::is_call_approved(target_cid, sender_addr.as_ref().unwrap()) {
            if !super::has_been_notified(target_cid, client_id) {
                info!(
                    "Sending CallRequest notification to target client {}",
                    target_cid
                );
                let call_req = ProtocolPacket::CallRequest {
                    caller_id: client_id,
                    caller_address: sender_addr.clone().unwrap_or_default(),
                };
                let target_dc = {
                    let reg = CLIENT_REGISTRY.read().unwrap();
                    reg.data_channels.get(&target_cid).cloned()
                };
                if let Some(tdc) = target_dc {
                    let _ = tdc.send(BytesMut::from(call_req.encode().as_slice())).await;
                }
                super::mark_notified(target_cid, client_id);
            }
            return;
        }

        let audio_msg = AudioMessage {
            sender_id: client_id,
            sender_address: sender_addr,
            target_address: target_address.clone(),
            data: audio_data,
        };
        let _ = AUDIO_BROADCAST.send(audio_msg);
    } else {
        crate::peer::forward_peer_audio(
            client_id,
            my_node_id,
            target_address,
            sender_addr.unwrap_or_default(),
            crate::constants::DEFAULT_INITIAL_TTL,
            audio_data,
        )
        .await;
    }
}

async fn handle_call_accept_response(client_id: u64, caller_address: UserAddress) {
    info!(
        "Client {} accepted call request from {}",
        client_id,
        caller_address.short_id()
    );
    super::mark_call_approved(client_id, caller_address.clone());

    if let Some(caller_cid) = super::find_client_by_address(&caller_address) {
        let caller_user_addr = CLIENT_REGISTRY
            .read()
            .unwrap()
            .client_addresses
            .get(&client_id)
            .cloned();

        let notif = ProtocolPacket::CallAcceptedNotification {
            target_address: caller_user_addr.unwrap_or_default(),
        };

        let caller_dc = {
            let reg = CLIENT_REGISTRY.read().unwrap();
            reg.data_channels.get(&caller_cid).cloned()
        };
        if let Some(cdc) = caller_dc {
            let _ = cdc.send(BytesMut::from(notif.encode().as_slice())).await;
        }
    }
}

async fn handle_call_reject_response(client_id: u64, caller_address: UserAddress) {
    info!(
        "Client {} rejected call request from {}",
        client_id,
        caller_address.short_id()
    );
    super::mark_call_rejected(client_id, caller_address.clone());

    if let Some(caller_cid) = super::find_client_by_address(&caller_address) {
        let caller_user_addr = CLIENT_REGISTRY
            .read()
            .unwrap()
            .client_addresses
            .get(&client_id)
            .cloned();

        let notif = ProtocolPacket::CallRejectedNotification {
            target_address: caller_user_addr.unwrap_or_default(),
        };

        let caller_dc = {
            let reg = CLIENT_REGISTRY.read().unwrap();
            reg.data_channels.get(&caller_cid).cloned()
        };
        if let Some(cdc) = caller_dc {
            let _ = cdc.send(BytesMut::from(notif.encode().as_slice())).await;
        }
    }
}

async fn handle_call_hangup(
    client_id: u64,
    sender_addr: Option<UserAddress>,
    target_address: UserAddress,
) {
    info!(
        "Client {} sent hangup signal for target {}",
        client_id,
        target_address.short_id()
    );

    if let Some(sender_addr) = sender_addr {
        CLIENT_REGISTRY
            .write()
            .unwrap()
            .remove_call_approval(client_id, &target_address);

        if let Some(target_cid) = super::find_client_by_address(&target_address) {
            CLIENT_REGISTRY
                .write()
                .unwrap()
                .remove_call_approval(target_cid, &sender_addr);

            let notif = ProtocolPacket::CallEndedNotification {
                target_address: sender_addr,
            };

            let target_dc = {
                let reg = CLIENT_REGISTRY.read().unwrap();
                reg.data_channels.get(&target_cid).cloned()
            };
            if let Some(tdc) = target_dc {
                let _ = tdc.send(BytesMut::from(notif.encode().as_slice())).await;
            }
        }
    }
}

async fn handle_room_join(client_id: u64, room_address: UserAddress) {
    info!(
        "Client {} joining room {}",
        client_id,
        room_address.short_id()
    );

    let (member_count, state_notif, old_members) = {
        let mut reg = CLIENT_REGISTRY.write().unwrap();
        let member_count = reg.get_room_participant_count(&room_address);

        if member_count >= crate::constants::MAX_ROOM_MEMBERS {
            warn!(
                "Client {} failed to join room {}: maximum capacity ({}) reached",
                client_id,
                room_address.short_id(),
                crate::constants::MAX_ROOM_MEMBERS
            );
            return;
        }

        let old_members = reg.get_room_members(&room_address);
        reg.join_room(client_id, room_address.clone());
        let new_count = reg.get_room_participant_count(&room_address);

        let state_notif = ProtocolPacket::RoomStateNotification {
            room_address: room_address.clone(),
            participant_count: new_count as u32,
        };

        (new_count, state_notif, old_members)
    };

    let notif_bytes = BytesMut::from(state_notif.encode().as_slice());

    let my_dc = {
        let reg = CLIENT_REGISTRY.read().unwrap();
        reg.data_channels.get(&client_id).cloned()
    };
    if let Some(dc) = my_dc {
        let _ = dc.send(notif_bytes.clone()).await;
    }

    for member_cid in old_members {
        let member_dc = {
            let reg = CLIENT_REGISTRY.read().unwrap();
            reg.data_channels.get(&member_cid).cloned()
        };
        if let Some(dc) = member_dc {
            let _ = dc.send(notif_bytes.clone()).await;
        }
    }

    info!(
        "Client {} joined room {} (total participants: {})",
        client_id,
        room_address.short_id(),
        member_count
    );
}

async fn handle_room_leave(client_id: u64, room_address: &UserAddress) {
    info!(
        "Client {} leaving room {}",
        client_id,
        room_address.short_id()
    );

    let (remaining_count, remaining_members) = {
        let mut reg = CLIENT_REGISTRY.write().unwrap();
        reg.leave_room(client_id, room_address);
        let count = reg.get_room_participant_count(room_address);
        let members = reg.get_room_members(room_address);
        (count, members)
    };

    let state_notif = ProtocolPacket::RoomStateNotification {
        room_address: room_address.clone(),
        participant_count: remaining_count as u32,
    };
    let notif_bytes = BytesMut::from(state_notif.encode().as_slice());

    for member_cid in remaining_members {
        let member_dc = {
            let reg = CLIENT_REGISTRY.read().unwrap();
            reg.data_channels.get(&member_cid).cloned()
        };
        if let Some(dc) = member_dc {
            let _ = dc.send(notif_bytes.clone()).await;
        }
    }
}

async fn handle_room_group_audio(
    client_id: u64,
    room_address: UserAddress,
    codec_id: u8,
    audio_data: Vec<u8>,
) {
    if !super::is_client_in_room(client_id, &room_address) {
        return;
    }

    let energy = super::calculate_audio_energy(&audio_data);
    let (room_members, top_speakers) = {
        let mut reg = CLIENT_REGISTRY.write().unwrap();
        reg.record_audio_energy(client_id, room_address.clone(), energy);
        let members = reg.get_room_members(&room_address);
        let top_speakers = reg.get_top_k_speakers(&room_address, crate::constants::TOP_K_SPEAKERS);
        (members, top_speakers)
    };

    let speaker_notice = ProtocolPacket::ActiveSpeakerNotice {
        room_address: room_address.clone(),
        speaker_addresses: top_speakers,
    };
    let speaker_bytes = BytesMut::from(speaker_notice.encode().as_slice());

    let my_user_addr = {
        let reg = CLIENT_REGISTRY.read().unwrap();
        reg.client_addresses.get(&client_id).cloned()
    };

    let server_audio_packet = ProtocolPacket::ServerTargetedAudio {
        target_address: room_address.clone(),
        sender_id: client_id,
        sender_address: my_user_addr.unwrap_or_default(),
        codec_id,
        audio_data,
    };
    let audio_bytes = BytesMut::from(server_audio_packet.encode().as_slice());

    for member_cid in room_members {
        let member_dc = {
            let reg = CLIENT_REGISTRY.read().unwrap();
            reg.data_channels.get(&member_cid).cloned()
        };

        if let Some(dc) = member_dc {
            let _ = dc.send(speaker_bytes.clone()).await;
            if member_cid != client_id {
                let _ = dc.send(audio_bytes.clone()).await;
            }
        }
    }
}
