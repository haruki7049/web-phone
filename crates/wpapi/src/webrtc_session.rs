//! WebRTC PeerConnection and DataChannel session setup module.
//!
//! Handles WebRTC API setup, ICE configuration, DataChannel message handling,
//! and SDP signaling handshake with the server.

use crate::address::UserAddress;
use crate::config::Configuration;
use crate::protocol::ProtocolPacket;
use crate::session::ClientSession;
use anyhow::{Result, anyhow};
use bytes::Bytes;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
use tracing::{error, info};
use webrtc::api::APIBuilder;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

/// Create and configure a WebRTC PeerConnection instance.
pub async fn create_peer_connection(config: &Configuration) -> Result<Arc<RTCPeerConnection>> {
    let api = APIBuilder::new().build();
    let rtc_config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec![config.stun_server.clone()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let peer_connection = Arc::new(api.new_peer_connection(rtc_config).await?);
    Ok(peer_connection)
}

/// Create and attach 'audio' DataChannel to PeerConnection.
pub async fn setup_data_channel(
    peer_connection: &Arc<RTCPeerConnection>,
    target_address: Option<UserAddress>,
    mut rx_audio: mpsc::Receiver<Vec<u8>>,
    config: &Configuration,
    session: &ClientSession,
) -> Result<Arc<RTCDataChannel>> {
    let my_client_id = Arc::clone(&session.client_id);
    let audio_buffer = Arc::clone(&session.audio_buffer);
    let data_channel = peer_connection.create_data_channel("audio", None).await?;
    session.set_data_channel(Arc::clone(&data_channel));

    if let Some(target) = target_address {
        session.set_target_address(Some(target));
    }

    let dc_clone = Arc::clone(&data_channel);
    let session_ref_open = session.clone();
    data_channel.on_open(Box::new(move || {
        let dc_inner = Arc::clone(&dc_clone);
        let session_ref = session_ref_open.clone();
        Box::pin(async move {
            info!("WebRTC DataChannel 'audio' successfully opened");

            if let Some(target) = session_ref.get_target_address() {
                let init_packet = ProtocolPacket::ClientTargetedAudio {
                    target_address: target,
                    codec_id: crate::protocol::CODEC_OPUS,
                    audio_data: vec![],
                };
                let _ = dc_inner.send(&Bytes::from(init_packet.encode())).await;
            }

            tokio::spawn(async move {
                while let Some(audio_bytes) = rx_audio.recv().await {
                    if let Some(target) = session_ref.get_target_address() {
                        let packet = ProtocolPacket::ClientTargetedAudio {
                            target_address: target,
                            codec_id: crate::protocol::CODEC_OPUS,
                            audio_data: audio_bytes,
                        };
                        if dc_inner.send(&Bytes::from(packet.encode())).await.is_err() {
                            break;
                        }
                    } else if let Some(room) = session_ref.get_room_address() {
                        let packet = ProtocolPacket::RoomGroupAudio {
                            room_address: room,
                            codec_id: crate::protocol::CODEC_OPUS,
                            audio_data: audio_bytes,
                        };
                        if dc_inner.send(&Bytes::from(packet.encode())).await.is_err() {
                            break;
                        }
                    }
                }
            });
        })
    }));

    let auto_accept = config.auto_accept;
    let allow_echoback = config.allow_echoback;
    let dc_msg = Arc::clone(&data_channel);
    let session_user_addr = Arc::clone(&session.user_address);
    let session_ref = session.clone();

    data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
        let dc_inner = Arc::clone(&dc_msg);
        let my_client_id = Arc::clone(&my_client_id);
        let audio_buffer = Arc::clone(&audio_buffer);
        let session_user_addr = Arc::clone(&session_user_addr);
        let session_ref = session_ref.clone();
        Box::pin(async move {
            let Ok(packet) = ProtocolPacket::decode(&msg.data) else {
                return;
            };

            match packet {
                ProtocolPacket::ClientAssignment {
                    client_id,
                    user_address,
                } => {
                    my_client_id.store(client_id, Ordering::SeqCst);
                    if let Ok(mut guard) = session_user_addr.lock() {
                        *guard = Some(user_address.clone());
                    }
                    info!("============================================================");
                    info!(" Assigned Temporary User ID (SHA-256): {}", user_address);
                    info!(" Short ID: {}", user_address.short_id());
                    info!(" Client ID: {}", client_id);
                    info!("============================================================");
                }
                ProtocolPacket::BroadcastAudio {
                    sender_id,
                    audio_data,
                } => {
                    let my_id = my_client_id.load(Ordering::SeqCst);
                    if !allow_echoback && sender_id == my_id {
                        return;
                    }
                    let samples: Vec<f32> = audio_data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|chunk| f32::from_le_bytes(*chunk))
                        .collect();

                    let mut buffer = audio_buffer.lock().unwrap();
                    for sample in samples {
                        buffer.push_back(sample);
                    }
                    if buffer.len() > 4800 {
                        let excess = buffer.len() - 4800;
                        for _ in 0..excess {
                            buffer.pop_front();
                        }
                    }
                }
                ProtocolPacket::ServerTargetedAudio {
                    sender_id,
                    sender_address,
                    audio_data,
                    ..
                } => {
                    let my_id = my_client_id.load(Ordering::SeqCst);
                    if !allow_echoback && sender_id == my_id {
                        return;
                    }
                    tracing::trace!("Received audio frame from {}", sender_address.short_id());
                    let samples: Vec<f32> = audio_data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|chunk| f32::from_le_bytes(*chunk).clamp(-1.0, 1.0))
                        .collect();

                    let mut buffer = audio_buffer.lock().unwrap();
                    for sample in samples {
                        buffer.push_back(sample);
                    }
                    if buffer.len() > 4800 {
                        let excess = buffer.len() - 4800;
                        for _ in 0..excess {
                            buffer.pop_front();
                        }
                    }
                }
                ProtocolPacket::CallRequest { caller_address, .. } => {
                    let caller_bytes = caller_address.to_bytes();
                    let caller_addr_clone = caller_address.clone();
                    let session_inner = session_ref.clone();
                    if let Some(incoming_tx) = session_ref.get_incoming_call_handler() {
                        let dc_reply = Arc::clone(&dc_inner);
                        tokio::spawn(async move {
                            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<bool>();
                            if incoming_tx
                                .send((caller_addr_clone.clone(), reply_tx))
                                .await
                                .is_ok()
                            {
                                let accepted = reply_rx.await.unwrap_or(false);
                                if accepted {
                                    info!(
                                        "Accepted incoming call request from {}",
                                        caller_addr_clone.short_id()
                                    );
                                    session_inner.set_target_address(Some(caller_addr_clone.clone()));
                                    let resp = ProtocolPacket::CallAcceptResponse {
                                        caller_address: UserAddress::from_bytes(caller_bytes),
                                    };
                                    let _ = dc_reply.send(&Bytes::from(resp.encode())).await;
                                } else {
                                    info!(
                                        "Rejected incoming call request from {}",
                                        caller_addr_clone.short_id()
                                    );
                                    let resp = ProtocolPacket::CallRejectResponse {
                                        caller_address: UserAddress::from_bytes(caller_bytes),
                                    };
                                    let _ = dc_reply.send(&Bytes::from(resp.encode())).await;
                                }
                            }
                        });
                    } else if auto_accept {
                        info!(
                            "Auto-accepting incoming call request from {} (Short ID: {})",
                            caller_address,
                            caller_address.short_id()
                        );
                        session_ref.set_target_address(Some(caller_address.clone()));
                        let resp = ProtocolPacket::CallAcceptResponse { caller_address };
                        let _ = dc_inner.send(&Bytes::from(resp.encode())).await;
                    } else {
                        let dc_reply = Arc::clone(&dc_inner);

                        tokio::spawn(async move {
                            use std::io::{self, Write};
                            println!(
                                "\n============================================================"
                            );
                            println!(" Incoming Call Request!");
                            println!(" From: {}", caller_addr_clone);
                            println!(" Short ID: {}", caller_addr_clone.short_id());
                            print!(" Allow connection? [y/N]: ");
                            let _ = io::stdout().flush();

                            let mut input = String::new();
                            let accepted = tokio::task::spawn_blocking(move || {
                                let stdin = io::stdin();
                                if stdin.read_line(&mut input).is_ok() {
                                    let trimmed = input.trim().to_lowercase();
                                    trimmed == "y" || trimmed == "yes"
                                } else {
                                    false
                                }
                            })
                            .await
                            .unwrap_or_default();

                            if accepted {
                                info!(
                                    "Accepted incoming call request from {}",
                                    caller_addr_clone.short_id()
                                );
                                session_inner.set_target_address(Some(caller_addr_clone.clone()));
                                let resp = ProtocolPacket::CallAcceptResponse {
                                    caller_address: UserAddress::from_bytes(caller_bytes),
                                };
                                let _ = dc_reply.send(&Bytes::from(resp.encode())).await;
                            } else {
                                info!(
                                    "Rejected incoming call request from {}",
                                    caller_addr_clone.short_id()
                                );
                                let resp = ProtocolPacket::CallRejectResponse {
                                    caller_address: UserAddress::from_bytes(caller_bytes),
                                };
                                let _ = dc_reply.send(&Bytes::from(resp.encode())).await;
                            }
                        });
                    }
                }
                ProtocolPacket::CallRejectedNotification { target_address } => {
                    error!("============================================================");
                    error!(
                        " Call Connection Error: Connection rejected by target user ({})",
                        target_address.short_id()
                    );
                    error!(" Connection rejected.");
                    error!("============================================================");
                    session_ref.set_target_address(None);
                    if let Some(tx) = session_ref.get_call_notification_handler() {
                        let _ = tx
                            .send(crate::session::CallNotification::Rejected(target_address))
                            .await;
                    }
                }
                ProtocolPacket::CallAcceptedNotification { target_address } => {
                    info!("============================================================");
                    info!(
                        " Call connection accepted by target user ({})!",
                        target_address.short_id()
                    );
                    info!(" Call connected.");
                    info!("============================================================");
                    session_ref.set_target_address(Some(target_address.clone()));
                    if let Some(tx) = session_ref.get_call_notification_handler() {
                        let _ = tx
                            .send(crate::session::CallNotification::Accepted(target_address))
                            .await;
                    }
                }
                ProtocolPacket::ConnectionError { target_address } => {
                    error!("============================================================");
                    error!(
                        " Call Connection Error: Target user ({}) is unavailable or in another call.",
                        target_address.short_id()
                    );
                    error!(" Connection rejected.");
                    error!("============================================================");
                    session_ref.set_target_address(None);
                    if let Some(tx) = session_ref.get_call_notification_handler() {
                        let _ = tx
                            .send(crate::session::CallNotification::Error(
                                target_address,
                                "Target user is unavailable or in another call".to_string(),
                            ))
                            .await;
                    }
                }
                ProtocolPacket::CallHangup { target_address } => {
                    info!(
                        "Call hangup initiated for target {}",
                        target_address.short_id()
                    );
                    session_ref.set_target_address(None);
                    if let Some(tx) = session_ref.get_call_notification_handler() {
                        let _ = tx
                            .send(crate::session::CallNotification::Hangup(target_address))
                            .await;
                    }
                }
                ProtocolPacket::CallEndedNotification { target_address } => {
                    info!("============================================================");
                    info!(
                        " Call Session Ended by peer user ({})",
                        target_address.short_id()
                    );
                    info!(" Returned to Standby Mode.");
                    info!("============================================================");
                    session_ref.set_target_address(None);
                    if let Some(tx) = session_ref.get_call_notification_handler() {
                        let _ = tx
                            .send(crate::session::CallNotification::Hangup(target_address))
                            .await;
                    }
                }
                ProtocolPacket::Ping { timestamp } => {
                    let pong = ProtocolPacket::Pong { timestamp };
                    let _ = dc_inner.send(&Bytes::from(pong.encode())).await;
                }
                ProtocolPacket::Pong { .. } => {}
                ProtocolPacket::RoomStateNotification {
                    room_address,
                    participant_count,
                } => {
                    info!("============================================================");
                    info!(
                        " Room State Update for {}: {} active participant(s)",
                        room_address.short_id(),
                        participant_count
                    );
                    info!("============================================================");
                }
                ProtocolPacket::ActiveSpeakerNotice {
                    room_address,
                    speaker_addresses,
                } => {
                    let short_ids: Vec<&str> =
                        speaker_addresses.iter().map(|a| a.short_id()).collect();
                    info!(
                        "Active speaker update in room {}: {:?}",
                        room_address.short_id(),
                        short_ids
                    );
                }
                ProtocolPacket::RoomGroupAudio { audio_data, .. } => {
                    let samples: Vec<f32> = audio_data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|chunk| f32::from_le_bytes(*chunk).clamp(-1.0, 1.0))
                        .collect();

                    let mut buffer = audio_buffer.lock().unwrap();
                    for sample in samples {
                        buffer.push_back(sample);
                    }
                    if buffer.len() > 4800 {
                        let excess = buffer.len() - 4800;
                        for _ in 0..excess {
                            buffer.pop_front();
                        }
                    }
                }
                _ => {}
            }
        })
    }));

    Ok(data_channel)
}

/// Create and attach 'audio' DataChannel for group room session (WPIP-08).
pub async fn setup_room_data_channel(
    peer_connection: &Arc<RTCPeerConnection>,
    room_address: UserAddress,
    mut rx_audio: mpsc::Receiver<Vec<u8>>,
    config: &Configuration,
    session: &ClientSession,
) -> Result<Arc<RTCDataChannel>> {
    let my_client_id = Arc::clone(&session.client_id);
    let audio_buffer = Arc::clone(&session.audio_buffer);
    let data_channel = peer_connection.create_data_channel("audio", None).await?;

    let dc_clone = Arc::clone(&data_channel);
    let r_addr_clone = room_address.clone();
    data_channel.on_open(Box::new(move || {
        let dc_inner = Arc::clone(&dc_clone);
        let room_addr = r_addr_clone.clone();
        Box::pin(async move {
            info!(
                "WebRTC DataChannel 'audio' successfully opened for Room {}",
                room_addr.short_id()
            );

            // 1. Send RoomJoinRequest (0x0D)
            let join_packet = ProtocolPacket::RoomJoinRequest {
                room_address: room_addr.clone(),
            };
            let _ = dc_inner.send(&Bytes::from(join_packet.encode())).await;

            // 2. Forward microphone audio as RoomGroupAudio (0x10)
            tokio::spawn(async move {
                while let Some(audio_bytes) = rx_audio.recv().await {
                    let packet = ProtocolPacket::RoomGroupAudio {
                        room_address: room_addr.clone(),
                        codec_id: crate::protocol::CODEC_OPUS,
                        audio_data: audio_bytes,
                    };
                    if dc_inner.send(&Bytes::from(packet.encode())).await.is_err() {
                        break;
                    }
                }
            });
        })
    }));

    let _allow_echoback = config.allow_echoback;
    let dc_msg = Arc::clone(&data_channel);
    let session_user_addr = Arc::clone(&session.user_address);

    data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
        let dc_inner = Arc::clone(&dc_msg);
        let my_client_id = Arc::clone(&my_client_id);
        let audio_buffer = Arc::clone(&audio_buffer);
        let session_user_addr = Arc::clone(&session_user_addr);
        Box::pin(async move {
            let Ok(packet) = ProtocolPacket::decode(&msg.data) else {
                return;
            };

            match packet {
                ProtocolPacket::ClientAssignment {
                    client_id,
                    user_address,
                } => {
                    my_client_id.store(client_id, Ordering::SeqCst);
                    if let Ok(mut guard) = session_user_addr.lock() {
                        *guard = Some(user_address.clone());
                    }
                    info!("============================================================");
                    info!(" Assigned Temporary User ID (SHA-256): {}", user_address);
                    info!(" Short ID: {}", user_address.short_id());
                    info!(" Client ID: {}", client_id);
                    info!("============================================================");
                }
                ProtocolPacket::RoomStateNotification {
                    room_address,
                    participant_count,
                } => {
                    info!("============================================================");
                    info!(
                        " Room State Update for {}: {} active participant(s)",
                        room_address.short_id(),
                        participant_count
                    );
                    info!("============================================================");
                }
                ProtocolPacket::ActiveSpeakerNotice {
                    room_address,
                    speaker_addresses,
                } => {
                    let short_ids: Vec<&str> =
                        speaker_addresses.iter().map(|a| a.short_id()).collect();
                    info!(
                        "Active speaker update in room {}: {:?}",
                        room_address.short_id(),
                        short_ids
                    );
                }
                ProtocolPacket::RoomGroupAudio { audio_data, .. } => {
                    let samples: Vec<f32> = audio_data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|chunk| f32::from_le_bytes(*chunk).clamp(-1.0, 1.0))
                        .collect();

                    let mut buffer = audio_buffer.lock().unwrap();
                    for sample in samples {
                        buffer.push_back(sample);
                    }
                    if buffer.len() > 4800 {
                        let excess = buffer.len() - 4800;
                        for _ in 0..excess {
                            buffer.pop_front();
                        }
                    }
                }
                ProtocolPacket::Ping { timestamp } => {
                    let pong = ProtocolPacket::Pong { timestamp };
                    let _ = dc_inner.send(&Bytes::from(pong.encode())).await;
                }
                ProtocolPacket::Pong { .. } => {}
                _ => {}
            }
        })
    }));

    Ok(data_channel)
}

/// Perform SDP Offer creation and HTTP signaling exchange with the server.
pub async fn perform_sdp_handshake(
    peer_connection: &Arc<RTCPeerConnection>,
    config: &Configuration,
    session: &ClientSession,
) -> Result<()> {
    let offer = peer_connection.create_offer(None).await?;
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection.set_local_description(offer).await?;
    let _ = gather_complete.recv().await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or_else(|| anyhow!("Failed to get local SDP description"))?;

    let is_loopback = config.server_ip.is_loopback();
    let scheme = if is_loopback { "http" } else { "https" };
    let server_url = match config.server_ip {
        std::net::IpAddr::V4(ip) => format!("{}://{}:{}", scheme, ip, config.server_port),
        std::net::IpAddr::V6(ip) => format!("{}://[{}]:{}", scheme, ip, config.server_port),
    };

    let sdp_endpoint = format!("{}/sdp", server_url);
    info!("Sending signed SDP offer to {}...", sdp_endpoint);

    let client_keypair = &session.keypair;
    let (_, auth_header_val) =
        crate::address::build_authorization_header(client_keypair, &local_desc.sdp);

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let resp = client
        .post(&sdp_endpoint)
        .header(reqwest::header::AUTHORIZATION, auth_header_val)
        .json(&local_desc)
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "Server returned error status for SDP offer: {}",
            resp.status()
        );
    }

    let answer: RTCSessionDescription = resp.json().await?;
    peer_connection.set_remote_description(answer).await?;
    info!("SDP Answer successfully set on PeerConnection");

    Ok(())
}
