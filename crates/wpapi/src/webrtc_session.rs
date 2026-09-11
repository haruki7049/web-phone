//! WebRTC PeerConnection and DataChannel session setup module.
//!
//! Handles WebRTC API setup, ICE configuration, DataChannel message handling,
//! and SDP signaling handshake with the server.

use crate::address::UserAddress;
use crate::config::Configuration;
use crate::protocol::ProtocolPacket;
use crate::session::{CallNotification, ClientSession};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
use tracing::info;
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCIceServer, RTCSessionDescription,
};

/// Internal ICE gather completion handler for wpapi.
struct ApiEventHandler {
    gather_complete_tx: mpsc::Sender<()>,
}

#[async_trait]
impl PeerConnectionEventHandler for ApiEventHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        tracing::info!("ICE Gathering State changed: {:?}", state);
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_complete_tx.send(()).await;
        }
    }

    async fn on_ice_connection_state_change(
        &self,
        state: webrtc::peer_connection::RTCIceConnectionState,
    ) {
        tracing::info!("ICE Connection State changed: {:?}", state);
        if state == webrtc::peer_connection::RTCIceConnectionState::Failed {
            tracing::error!(
                "ICE Connection FAILED: WebRTC UDP media/DataChannel connection to daemon could not be established."
            );
        }
    }
}

/// Create and configure a WebRTC PeerConnection instance with custom handler.
pub async fn create_peer_connection_with_handler(
    config: &Configuration,
    handler: Arc<dyn PeerConnectionEventHandler>,
) -> Result<Arc<dyn PeerConnection>> {
    let rtc_config = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec![config.network.stun_server.clone()],
            ..Default::default()
        }])
        .build();

    let pc = PeerConnectionBuilder::new()
        .with_configuration(rtc_config)
        .with_handler(handler)
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await?;

    Ok(Arc::new(pc))
}

/// Create and configure a WebRTC PeerConnection instance.
pub async fn create_peer_connection(
    config: &Configuration,
) -> Result<(Arc<dyn PeerConnection>, mpsc::Receiver<()>)> {
    let (tx, rx) = mpsc::channel(1);
    let handler = Arc::new(ApiEventHandler {
        gather_complete_tx: tx,
    });
    let pc = create_peer_connection_with_handler(config, handler).await?;
    Ok((pc, rx))
}

/// Create and attach 'audio' DataChannel to PeerConnection.
pub async fn setup_data_channel(
    peer_connection: &Arc<dyn PeerConnection>,
    target_address: Option<UserAddress>,
    rx_audio: mpsc::Receiver<Vec<u8>>,
    config: &Configuration,
    session: &ClientSession,
) -> Result<Arc<dyn DataChannel>> {
    let audio_buffer = Arc::clone(&session.audio_buffer);
    let data_channel = peer_connection.create_data_channel("audio", None).await?;
    session.set_data_channel(Arc::clone(&data_channel));

    let target_addr_init = target_address.clone();
    let dc_init = Arc::clone(&data_channel);
    let session_ref = session.clone();
    let auto_accept = config.client.auto_accept;
    let mut rx_audio_opt = Some(rx_audio);

    tokio::spawn(async move {
        while let Some(event) = dc_init.poll().await {
            match event {
                DataChannelEvent::OnOpen => {
                    info!("WebRTC DataChannel 'audio' successfully opened.");

                    if let Some(ref target) = target_addr_init {
                        let init_packet = ProtocolPacket::ClientTargetedAudio {
                            target_address: target.clone(),
                            codec_id: crate::protocol::CODEC_OPUS,
                            audio_data: vec![],
                        };
                        let _ = dc_init
                            .send(BytesMut::from(init_packet.encode().as_slice()))
                            .await;
                    }

                    if let Some(mut rx) = rx_audio_opt.take() {
                        let dc_inner = Arc::clone(&dc_init);
                        let sess_target = session_ref.clone();
                        tokio::spawn(async move {
                            while let Some(audio_bytes) = rx.recv().await {
                                if let Some(target) = sess_target.get_target_address() {
                                    let packet = ProtocolPacket::ClientTargetedAudio {
                                        target_address: target,
                                        codec_id: crate::protocol::CODEC_OPUS,
                                        audio_data: audio_bytes,
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
                        });
                    }
                }
                DataChannelEvent::OnMessage(msg) => {
                    let Ok(packet) = ProtocolPacket::decode(&msg.data) else {
                        continue;
                    };

                    dispatch_client_packet(
                        packet,
                        &session_ref,
                        auto_accept,
                        &dc_init,
                        &audio_buffer,
                    )
                    .await;
                }
                DataChannelEvent::OnClose => {
                    info!("DataChannel closed.");
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(data_channel)
}

async fn dispatch_client_packet(
    packet: ProtocolPacket,
    session_ref: &ClientSession,
    auto_accept: bool,
    dc_init: &Arc<dyn DataChannel>,
    audio_buffer: &Arc<crate::audio::AudioRingBuffer>,
) {
    match packet {
        ProtocolPacket::ClientAssignment {
            client_id,
            user_address,
        } => {
            info!(
                "Assigned client ID: {} | UserAddress: {}",
                client_id, user_address
            );
            session_ref.client_id.store(client_id, Ordering::SeqCst);
            if let Ok(mut guard) = session_ref.user_address.lock() {
                *guard = Some(user_address);
            }
        }
        ProtocolPacket::CallRequest { caller_address, .. } => {
            info!("Incoming Call Request from {}", caller_address.short_id());
            let my_addr_opt = session_ref.get_user_address();
            let is_self_call = my_addr_opt
                .as_ref()
                .map(|my_addr| {
                    my_addr == &caller_address
                        || my_addr.short_id() == caller_address.short_id()
                        || my_addr.id.starts_with(caller_address.short_id())
                        || caller_address.id.starts_with(my_addr.short_id())
                })
                .unwrap_or(false);

            if is_self_call {
                info!(
                    "Ignoring self-incoming CallRequest from {}",
                    caller_address.short_id()
                );
            } else {
                let current_target = session_ref.get_target_address();
                let is_already_calling_target = current_target
                    .as_ref()
                    .map(|tgt| {
                        tgt == &caller_address
                            || tgt.short_id() == caller_address.short_id()
                            || tgt.id.starts_with(caller_address.short_id())
                            || caller_address.id.starts_with(tgt.short_id())
                    })
                    .unwrap_or(false);

                if auto_accept || is_already_calling_target {
                    info!(
                        "Auto-accepting call from {} (auto_accept: {}, is_already_calling: {})",
                        caller_address.short_id(),
                        auto_accept,
                        is_already_calling_target
                    );
                    session_ref.set_target_address(Some(caller_address.clone()));
                    let accept = ProtocolPacket::CallAcceptResponse {
                        caller_address: caller_address.clone(),
                    };
                    let _ = dc_init
                        .send(BytesMut::from(accept.encode().as_slice()))
                        .await;
                } else if let Some(tx) = session_ref.get_incoming_call_handler() {
                    session_ref.set_target_address(Some(caller_address.clone()));
                    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
                    let dc_reply = Arc::clone(dc_init);
                    let caller_addr = caller_address.clone();
                    tokio::spawn(async move {
                        if let Ok(accepted) = resp_rx.await {
                            let packet = if accepted {
                                ProtocolPacket::CallAcceptResponse {
                                    caller_address: caller_addr,
                                }
                            } else {
                                ProtocolPacket::CallRejectResponse {
                                    caller_address: caller_addr,
                                }
                            };
                            let _ = dc_reply
                                .send(BytesMut::from(packet.encode().as_slice()))
                                .await;
                        }
                    });
                    let _ = tx.send((caller_address, resp_tx)).await;
                }
            }
        }
        ProtocolPacket::CallAcceptResponse { caller_address } => {
            info!("Call accepted by {}", caller_address.short_id());
            session_ref.set_target_address(Some(caller_address.clone()));
            if let Some(tx) = session_ref.get_call_notification_handler() {
                let _ = tx.send(CallNotification::Accepted(caller_address)).await;
            }
        }
        ProtocolPacket::CallRejectResponse { caller_address } => {
            info!("Call rejected by {}", caller_address.short_id());
            session_ref.set_target_address(None);
            if let Some(tx) = session_ref.get_call_notification_handler() {
                let _ = tx.send(CallNotification::Rejected(caller_address)).await;
            }
        }
        ProtocolPacket::CallAcceptedNotification { target_address } => {
            info!(
                "Call accepted notification for {}",
                target_address.short_id()
            );
            session_ref.set_target_address(Some(target_address.clone()));
            if let Some(tx) = session_ref.get_call_notification_handler() {
                let _ = tx.send(CallNotification::Accepted(target_address)).await;
            }
        }
        ProtocolPacket::CallRejectedNotification { target_address } => {
            info!(
                "Call rejected notification for {}",
                target_address.short_id()
            );
            session_ref.set_target_address(None);
            if let Some(tx) = session_ref.get_call_notification_handler() {
                let _ = tx.send(CallNotification::Rejected(target_address)).await;
            }
        }
        ProtocolPacket::ConnectionError { target_address } => {
            info!("Connection error for target {}", target_address.short_id());
            session_ref.set_target_address(None);
            if let Some(tx) = session_ref.get_call_notification_handler() {
                let _ = tx
                    .send(CallNotification::Error(
                        target_address,
                        "Target user not found or offline".to_string(),
                    ))
                    .await;
            }
        }
        ProtocolPacket::CallEndedNotification { target_address } => {
            info!("Call ended notification from {}", target_address.short_id());
            session_ref.set_target_address(None);
            if let Some(tx) = session_ref.get_call_notification_handler() {
                let _ = tx
                    .send(CallNotification::Error(
                        target_address,
                        "Call ended by remote user".to_string(),
                    ))
                    .await;
            }
        }
        ProtocolPacket::ServerTargetedAudio { audio_data, .. }
        | ProtocolPacket::PeerTargetedAudio { audio_data, .. } => {
            let samples: Vec<f32> = audio_data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| f32::from_le_bytes(*chunk).clamp(-1.0, 1.0))
                .collect();

            audio_buffer.push_slice(&samples);
        }
        ProtocolPacket::Ping { timestamp } => {
            let pong = ProtocolPacket::Pong { timestamp };
            let _ = dc_init.send(BytesMut::from(pong.encode().as_slice())).await;
        }
        ProtocolPacket::Pong { .. } => {}
        _ => {}
    }
}

/// Create and attach room 'audio' DataChannel to PeerConnection.
pub async fn setup_room_data_channel(
    peer_connection: &Arc<dyn PeerConnection>,
    room_address: UserAddress,
    rx_audio: mpsc::Receiver<Vec<u8>>,
    _config: &Configuration,
    session: &ClientSession,
) -> Result<Arc<dyn DataChannel>> {
    let my_client_id = Arc::clone(&session.client_id);
    let audio_buffer = Arc::clone(&session.audio_buffer);
    let data_channel = peer_connection.create_data_channel("audio", None).await?;

    let dc_task = Arc::clone(&data_channel);
    let session_user_addr = Arc::clone(&session.user_address);
    let r_addr_clone = room_address.clone();
    let mut rx_audio_opt = Some(rx_audio);

    tokio::spawn(async move {
        while let Some(event) = dc_task.poll().await {
            match event {
                DataChannelEvent::OnOpen => {
                    info!(
                        "WebRTC DataChannel 'audio' successfully opened for Room {}",
                        r_addr_clone.short_id()
                    );

                    let join_packet = ProtocolPacket::RoomJoinRequest {
                        room_address: r_addr_clone.clone(),
                    };
                    let _ = dc_task
                        .send(BytesMut::from(join_packet.encode().as_slice()))
                        .await;

                    if let Some(mut rx) = rx_audio_opt.take() {
                        let dc_inner = Arc::clone(&dc_task);
                        let r_addr = r_addr_clone.clone();
                        tokio::spawn(async move {
                            while let Some(audio_bytes) = rx.recv().await {
                                let packet = ProtocolPacket::RoomGroupAudio {
                                    room_address: r_addr.clone(),
                                    codec_id: crate::protocol::CODEC_OPUS,
                                    audio_data: audio_bytes,
                                };
                                if dc_inner
                                    .send(BytesMut::from(packet.encode().as_slice()))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        });
                    }
                }
                DataChannelEvent::OnMessage(msg) => {
                    let Ok(packet) = ProtocolPacket::decode(&msg.data) else {
                        continue;
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

                            audio_buffer.push_slice(&samples);
                        }
                        ProtocolPacket::Ping { timestamp } => {
                            let pong = ProtocolPacket::Pong { timestamp };
                            let _ = dc_task.send(BytesMut::from(pong.encode().as_slice())).await;
                        }
                        ProtocolPacket::Pong { .. } => {}
                        _ => {}
                    }
                }
                DataChannelEvent::OnClose => {
                    info!("Room DataChannel closed.");
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(data_channel)
}

/// Perform SDP Offer creation and HTTP signaling exchange with the server.
pub async fn perform_sdp_handshake(
    peer_connection: &Arc<dyn PeerConnection>,
    config: &Configuration,
    session: &ClientSession,
    mut gather_rx: mpsc::Receiver<()>,
) -> Result<()> {
    let offer = peer_connection.create_offer(None).await?;
    peer_connection.set_local_description(offer).await?;

    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), gather_rx.recv()).await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or_else(|| anyhow!("Failed to get local SDP description"))?;

    let server_url = config.server_url();

    let sdp_endpoint = format!("{}/sdp", server_url);
    info!("Sending signed SDP offer to {}...", sdp_endpoint);

    let client_keypair = &session.keypair;
    let (_, auth_header_val) =
        crate::address::build_authorization_header(client_keypair, &local_desc.sdp);

    let http_client = reqwest::Client::builder().build()?;
    let resp = match http_client
        .post(&sdp_endpoint)
        .header("Content-Type", "application/json")
        .header(reqwest::header::AUTHORIZATION, auth_header_val.clone())
        .header("X-WebPhone-Sign", auth_header_val)
        .json(&local_desc)
        .send()
        .await
    {
        Ok(res) => res,
        Err(err) => {
            tracing::error!(
                "Failed to send SDP offer HTTP POST to {}: {}",
                sdp_endpoint,
                err
            );
            return Err(anyhow!(
                "Failed to send SDP offer to {}: {}",
                sdp_endpoint,
                err
            ));
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Server rejected SDP offer with status {}: {}",
            status,
            err_body
        ));
    }

    let answer: RTCSessionDescription = resp.json().await?;
    info!("Received SDP answer from server. Setting remote description...");
    peer_connection.set_remote_description(answer).await?;
    info!("WebRTC SDP handshake completed successfully.");

    Ok(())
}
