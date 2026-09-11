//! Inter-daemon peer mesh module.
//!
//! This module allows a `wpdaemon` instance to connect to other `wpdaemon` instances,
//! forming an interconnected daemon mesh that relays audio and client information
//! across multiple daemon servers.

use crate::broadcast::{AUDIO_BROADCAST, AudioMessage};
use crate::config::CONFIGURATION;
use crate::constants::ICE_GATHER_TIMEOUT;
use crate::registry::CLIENT_REGISTRY;
use async_trait::async_trait;
use axum::extract::Json;
use bytes::BytesMut;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::mpsc;
use tracing::{info, warn};
use webrtc::data_channel::{DataChannel, DataChannelEvent};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCSessionDescription,
};

/// Active inter-daemon peer connections.
static PEER_DAEMONS: LazyLock<Mutex<HashMap<String, Arc<dyn PeerConnection>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct MeshPeerEventHandler {
    gather_tx: mpsc::Sender<()>,
}

/// Helper to handle audio broadcast relay to a peer DataChannel (WPIP-05).
async fn process_peer_audio_send_loop(
    dc: Arc<dyn DataChannel>,
    mut rx: tokio::sync::broadcast::Receiver<AudioMessage>,
    my_node_id: u64,
) {
    while let Ok(audio_msg) = rx.recv().await {
        if audio_msg.origin_node != my_node_id && audio_msg.ttl == 0 {
            continue;
        }
        let target_addr = audio_msg.target_address.clone();
        let sender_addr = audio_msg.sender_address.unwrap_or_default();

        let packet = wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            sender_id: audio_msg.sender_id,
            origin_node: audio_msg.origin_node,
            target_address: target_addr,
            sender_address: sender_addr,
            codec_id: wpapi::protocol::CODEC_OPUS,
            ttl: audio_msg.ttl,
            audio_data: audio_msg.data,
        };

        if dc
            .send(BytesMut::from(packet.encode().as_slice()))
            .await
            .is_err()
        {
            break;
        }
    }
}

/// Helper to handle incoming PeerTargetedAudio packets with WPIP-05 loop prevention and TTL checks.
fn handle_peer_incoming_message(msg_data: &[u8], my_node_id: u64) {
    if let Ok(wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
        sender_id,
        origin_node,
        target_address,
        sender_address,
        ttl,
        audio_data,
        ..
    }) = wpapi::protocol::ProtocolPacket::decode(msg_data)
    {
        // WPIP-05 Loop prevention & TTL expiration checks
        if origin_node != my_node_id && ttl > 0 {
            let next_ttl = ttl - 1;
            let _ = AUDIO_BROADCAST.send(AudioMessage {
                sender_id,
                sender_address: Some(sender_address),
                target_address,
                origin_node,
                ttl: next_ttl,
                data: audio_data,
            });
        } else {
            warn!(
                "Dropped PeerTargetedAudio: origin_node loop ({}) or TTL expired ({})",
                origin_node == my_node_id,
                ttl == 0
            );
        }
    }
}

#[async_trait]
impl PeerConnectionEventHandler for MeshPeerEventHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            let _ = self.gather_tx.send(()).await;
        }
    }

    async fn on_data_channel(&self, dc: Arc<dyn DataChannel>) {
        let dc_task = Arc::clone(&dc);
        tokio::spawn(async move {
            let mut audio_rx_opt = Some(AUDIO_BROADCAST.subscribe());
            while let Some(event) = dc_task.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        info!("Inbound peer wpdaemon DataChannel opened");
                        if let Some(rx) = audio_rx_opt.take() {
                            let dc_inner = Arc::clone(&dc_task);
                            tokio::spawn(async move {
                                let config = CONFIGURATION.get().cloned().unwrap_or_default();
                                process_peer_audio_send_loop(dc_inner, rx, config.mesh.node_id)
                                    .await;
                            });
                        }
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        let config = CONFIGURATION.get().cloned().unwrap_or_default();
                        handle_peer_incoming_message(&msg.data, config.mesh.node_id);
                    }
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        });
    }
}

use crate::error::SignalingError;
use axum::http::HeaderMap;

/// Handle incoming SDP offer from another peer wpdaemon node.
pub async fn handle_peer_sdp(
    headers: HeaderMap,
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, SignalingError> {
    let config = CONFIGURATION.get().cloned().unwrap_or_default();

    // Peer authentication verification (WPIP-05 / Security Audit)
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .or_else(|| headers.get("X-Peer-Auth"))
        .and_then(|v| v.to_str().ok());

    if let Some(secret) = &config.mesh.peer_secret {
        let expected_bearer = format!("Bearer {}", secret);
        let expected_auth = format!("WP-PeerAuth {}", secret);
        let is_valid = auth_header
            .is_some_and(|hdr| hdr == secret || hdr == expected_bearer || hdr == expected_auth);
        if !is_valid {
            warn!("Rejected peer SDP offer: Invalid peer_secret token.");
            return Err(SignalingError::Unauthorized(
                wpapi::AuthError::InvalidTurnSignature,
            ));
        }
    } else if let Some(auth_val) = auth_header {
        if let Err(err) = wpapi::verify_any_authorization_header(auth_val, &offer.sdp) {
            warn!(
                "Rejected peer SDP offer: Signature verification failed ({})",
                err
            );
            return Err(SignalingError::Unauthorized(err));
        }
    } else if !config.server.allow_anonymous {
        warn!("Rejected peer SDP offer: Missing peer authorization header.");
        return Err(SignalingError::Unauthorized(
            wpapi::AuthError::MissingHeader,
        ));
    }

    let active_connections = CLIENT_REGISTRY.read().unwrap().peer_connections.len();
    if active_connections >= config.server.max_connections {
        warn!(
            "Rejected peer SDP offer: active connections ({}) reached max limit ({})",
            active_connections, config.server.max_connections
        );
        return Err(SignalingError::MaxConnectionsReached(
            config.server.max_connections,
        ));
    }

    let current_peers = PEER_DAEMONS.lock().unwrap().len();
    if current_peers >= config.mesh.max_mesh_peers {
        warn!(
            "Rejected peer SDP offer: mesh peer connections ({}) reached max limit ({})",
            current_peers, config.mesh.max_mesh_peers
        );
        return Err(SignalingError::MaxMeshPeersReached(
            config.mesh.max_mesh_peers,
        ));
    }

    let (gather_tx, mut gather_rx) = mpsc::channel(1);
    let handler = Arc::new(MeshPeerEventHandler { gather_tx });
    let rtc_config = RTCConfigurationBuilder::default().build();

    let pc = PeerConnectionBuilder::new()
        .with_configuration(rtc_config)
        .with_handler(handler)
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await
        .map_err(|e| {
            SignalingError::InternalError(format!("Failed to create PeerConnection: {}", e))
        })?;

    let peer_connection: Arc<dyn PeerConnection> = Arc::new(pc);

    info!("Received inbound peer wpdaemon connection request");

    peer_connection
        .set_remote_description(offer)
        .await
        .map_err(|e| SignalingError::InvalidSdpOffer(e.to_string()))?;

    let answer = peer_connection
        .create_answer(None)
        .await
        .map_err(|e| SignalingError::InternalError(format!("Failed to create answer: {}", e)))?;

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

/// Initiate connection to a target peer wpdaemon.
pub async fn connect_to_peer(
    peer_url: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = CONFIGURATION.get().cloned().unwrap_or_default();
    info!("Connecting to peer wpdaemon at {}...", peer_url);

    if peer_url.starts_with("http://")
        && !peer_url.contains("127.0.0.1")
        && !peer_url.contains("localhost")
        && !peer_url.contains("[::1]")
    {
        warn!(
            "SECURITY WARNING: Connecting to peer daemon {} using unencrypted HTTP. HTTPS transport security is strongly recommended for non-loopback mesh peers!",
            peer_url
        );
    }

    let (gather_tx, mut gather_rx) = mpsc::channel(1);
    let handler = Arc::new(MeshPeerEventHandler { gather_tx });
    let rtc_config = RTCConfigurationBuilder::default().build();

    let pc = PeerConnectionBuilder::new()
        .with_configuration(rtc_config)
        .with_handler(handler)
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await?;

    let peer_connection: Arc<dyn PeerConnection> = Arc::new(pc);

    let data_channel = peer_connection
        .create_data_channel("daemon-peer", None)
        .await?;

    let dc_task = Arc::clone(&data_channel);
    let peer_url_log = peer_url.clone();

    tokio::spawn(async move {
        let mut audio_rx_opt = Some(AUDIO_BROADCAST.subscribe());
        while let Some(event) = dc_task.poll().await {
            match event {
                DataChannelEvent::OnOpen => {
                    info!("Outbound peer DataChannel opened to {}", peer_url_log);
                    if let Some(rx) = audio_rx_opt.take() {
                        let dc_inner = Arc::clone(&dc_task);
                        tokio::spawn(async move {
                            let config = CONFIGURATION.get().cloned().unwrap_or_default();
                            process_peer_audio_send_loop(dc_inner, rx, config.mesh.node_id).await;
                        });
                    }
                }
                DataChannelEvent::OnMessage(msg) => {
                    let config = CONFIGURATION.get().cloned().unwrap_or_default();
                    handle_peer_incoming_message(&msg.data, config.mesh.node_id);
                }
                DataChannelEvent::OnClose => break,
                _ => {}
            }
        }
    });

    let offer = peer_connection.create_offer(None).await?;
    peer_connection.set_local_description(offer).await?;

    let _ = tokio::time::timeout(ICE_GATHER_TIMEOUT, gather_rx.recv()).await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or("No local description available")?;

    let client = reqwest::Client::new();
    let sdp_url = format!("{}/peer/sdp", peer_url.trim_end_matches('/'));

    let auth_header_val = if let Some(secret) = &config.mesh.peer_secret {
        format!("Bearer {}", secret)
    } else {
        let keypair = wpapi::UserKeypair::generate();
        let (_, hdr) = wpapi::build_authorization_header(&keypair, &local_desc.sdp);
        hdr
    };

    let response = client
        .post(&sdp_url)
        .header(reqwest::header::AUTHORIZATION, auth_header_val)
        .json(&local_desc)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Peer returned error status: {}", response.status()).into());
    }

    let answer: RTCSessionDescription = response.json().await?;
    peer_connection.set_remote_description(answer).await?;

    PEER_DAEMONS
        .lock()
        .unwrap()
        .insert(peer_url, peer_connection);

    info!("Successfully interconnected with peer wpdaemon");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wpapi::UserAddress;

    #[tokio::test]
    async fn test_peer_loop_prevention_origin_and_ttl() {
        let my_node_id = 99900u64;
        let mut rx = AUDIO_BROADCAST.subscribe();

        // Drain any messages sent by other concurrent tests
        while rx.try_recv().is_ok() {}

        // 1. Packet originating from my_node_id: Should be dropped (loop prevention)
        let loop_packet = wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            sender_id: 99901,
            origin_node: my_node_id,
            target_address: UserAddress::new("target_addr_12345"),
            sender_address: UserAddress::new("sender_addr_12345"),
            codec_id: wpapi::protocol::CODEC_OPUS,
            ttl: 8,
            audio_data: vec![1, 2, 3],
        };
        let encoded_loop = loop_packet.encode();
        handle_peer_incoming_message(&encoded_loop, my_node_id);
        while let Ok(msg) = rx.try_recv() {
            assert_ne!(
                msg.sender_id, 99901,
                "Loop packet should not be re-broadcasted"
            );
        }

        // 2. Packet with TTL == 0: Should be dropped
        let ttl_zero_packet = wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            sender_id: 99902,
            origin_node: 200u64,
            target_address: UserAddress::new("target_addr_12345"),
            sender_address: UserAddress::new("sender_addr_12345"),
            codec_id: wpapi::protocol::CODEC_OPUS,
            ttl: 0,
            audio_data: vec![4, 5, 6],
        };
        let encoded_zero = ttl_zero_packet.encode();
        handle_peer_incoming_message(&encoded_zero, my_node_id);
        while let Ok(msg) = rx.try_recv() {
            assert_ne!(
                msg.sender_id, 99902,
                "TTL 0 packet should not be re-broadcasted"
            );
        }

        // 3. Valid peer packet (origin_node != my_node_id, ttl = 8): Should decrement TTL to 7 and broadcast
        let valid_packet = wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            sender_id: 99903,
            origin_node: 300u64,
            target_address: UserAddress::new("target_addr_12345"),
            sender_address: UserAddress::new("sender_addr_12345"),
            codec_id: wpapi::protocol::CODEC_OPUS,
            ttl: 8,
            audio_data: vec![7, 8, 9],
        };
        let encoded_valid = valid_packet.encode();
        handle_peer_incoming_message(&encoded_valid, my_node_id);

        let mut found_valid = false;
        while let Ok(recv_msg) = rx.try_recv() {
            if recv_msg.sender_id == 99903 {
                assert_eq!(recv_msg.origin_node, 300u64);
                assert_eq!(recv_msg.ttl, 7); // Decremented from 8 to 7
                found_valid = true;
                break;
            }
        }
        assert!(
            found_valid,
            "Valid packet should be broadcasted with decremented TTL"
        );
    }
}
