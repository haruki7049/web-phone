//! Inter-daemon peer mesh module.
//!
//! This module allows a `wpdaemon` instance to connect to other `wpdaemon` instances,
//! forming an interconnected daemon mesh that relays audio and client information
//! across multiple daemon servers.

use crate::broadcast::{AUDIO_BROADCAST, AudioMessage};
use crate::config::CONFIGURATION;
use crate::registry::CLIENT_REGISTRY;
use async_trait::async_trait;
use axum::{extract::Json, http::StatusCode};
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
                        if let Some(mut rx) = audio_rx_opt.take() {
                            let dc_inner = Arc::clone(&dc_task);
                            tokio::spawn(async move {
                                let config = CONFIGURATION.get().cloned().unwrap_or_default();
                                let my_node_id = config.node_id;
                                loop {
                                    match rx.recv().await {
                                        Ok(audio_msg) => {
                                            if audio_msg.origin_node != my_node_id
                                                && audio_msg.ttl == 0
                                            {
                                                continue;
                                            }
                                            let target_addr = audio_msg.target_address.clone();
                                            let sender_addr =
                                                audio_msg.sender_address.unwrap_or_default();

                                            let packet = wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
                                                sender_id: audio_msg.sender_id,
                                                origin_node: audio_msg.origin_node,
                                                target_address: target_addr,
                                                sender_address: sender_addr,
                                                codec_id: wpapi::protocol::CODEC_OPUS,
                                                ttl: audio_msg.ttl,
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
                                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                            break;
                                        }
                                        Err(tokio::sync::broadcast::error::RecvError::Lagged(
                                            _,
                                        )) => {
                                            continue;
                                        }
                                    }
                                }
                            });
                        }
                    }
                    DataChannelEvent::OnMessage(msg) => {
                        let config = CONFIGURATION.get().cloned().unwrap_or_default();
                        let my_node_id = config.node_id;

                        if let Ok(wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
                            sender_id,
                            origin_node,
                            target_address,
                            sender_address,
                            ttl,
                            audio_data,
                            ..
                        }) = wpapi::protocol::ProtocolPacket::decode(&msg.data)
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
                    DataChannelEvent::OnClose => break,
                    _ => {}
                }
            }
        });
    }
}

/// Handle incoming SDP offer from another peer wpdaemon node.
pub async fn handle_peer_sdp(
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, (StatusCode, String)> {
    let config = CONFIGURATION.get().cloned().unwrap_or_default();

    let active_connections = CLIENT_REGISTRY.read().unwrap().peer_connections.len();
    if active_connections >= config.max_connections {
        warn!(
            "Rejected peer SDP offer: active connections ({}) reached max limit ({})",
            active_connections, config.max_connections
        );
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "503 Service Unavailable: Maximum concurrent connections reached ({})",
                config.max_connections
            ),
        ));
    }

    let current_peers = PEER_DAEMONS.lock().unwrap().len();
    if current_peers >= config.max_mesh_peers {
        warn!(
            "Rejected peer SDP offer: mesh peer connections ({}) reached max limit ({})",
            current_peers, config.max_mesh_peers
        );
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "503 Service Unavailable: Maximum mesh peer connections reached ({})",
                config.max_mesh_peers
            ),
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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create PeerConnection: {}", e),
            )
        })?;

    let peer_connection: Arc<dyn PeerConnection> = Arc::new(pc);

    info!("Received inbound peer wpdaemon connection request");

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

    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), gather_rx.recv()).await;

    let local_desc = peer_connection.local_description().await.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "No local description available".to_string(),
        )
    })?;

    Ok(Json(local_desc))
}

/// Initiate connection to a target peer wpdaemon.
pub async fn connect_to_peer(
    peer_url: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = CONFIGURATION.get().cloned().unwrap_or_default();
    let my_node_id = config.node_id;

    info!("Connecting to peer wpdaemon at {}...", peer_url);

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
                    if let Some(mut rx) = audio_rx_opt.take() {
                        let dc_inner = Arc::clone(&dc_task);
                        tokio::spawn(async move {
                            let config = CONFIGURATION.get().cloned().unwrap_or_default();
                            let my_node_id = config.node_id;
                            loop {
                                match rx.recv().await {
                                    Ok(audio_msg) => {
                                        if audio_msg.origin_node != my_node_id && audio_msg.ttl == 0
                                        {
                                            continue;
                                        }
                                        let target_addr = audio_msg.target_address.clone();
                                        let sender_addr =
                                            audio_msg.sender_address.unwrap_or_default();

                                        let packet =
                                            wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
                                                sender_id: audio_msg.sender_id,
                                                origin_node: audio_msg.origin_node,
                                                target_address: target_addr,
                                                sender_address: sender_addr,
                                                codec_id: wpapi::protocol::CODEC_OPUS,
                                                ttl: audio_msg.ttl,
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
                    if let Ok(wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
                        sender_id,
                        origin_node,
                        target_address,
                        sender_address,
                        ttl,
                        audio_data,
                        ..
                    }) = wpapi::protocol::ProtocolPacket::decode(&msg.data)
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
                DataChannelEvent::OnClose => break,
                _ => {}
            }
        }
    });

    let offer = peer_connection.create_offer(None).await?;
    peer_connection.set_local_description(offer).await?;

    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), gather_rx.recv()).await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or("No local description available")?;

    let client = reqwest::Client::new();
    let sdp_url = format!("{}/peer/sdp", peer_url.trim_end_matches('/'));

    let response = client.post(&sdp_url).json(&local_desc).send().await?;

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
        let my_node_id = 100u64;
        let mut rx = AUDIO_BROADCAST.subscribe();

        // 1. Packet originating from my_node_id: Should be dropped (loop prevention)
        let loop_packet = wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            sender_id: 1,
            origin_node: my_node_id,
            target_address: UserAddress::new("target_addr_12345"),
            sender_address: UserAddress::new("sender_addr_12345"),
            codec_id: wpapi::protocol::CODEC_OPUS,
            ttl: 8,
            audio_data: vec![1, 2, 3],
        };
        let encoded_loop = loop_packet.encode();

        if let Ok(wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            origin_node, ttl, ..
        }) = wpapi::protocol::ProtocolPacket::decode(&encoded_loop)
        {
            if origin_node != my_node_id && ttl > 0 {
                let _ = AUDIO_BROADCAST.send(AudioMessage {
                    sender_id: 1,
                    sender_address: None,
                    target_address: UserAddress::new("target_addr_12345"),
                    origin_node,
                    ttl: ttl - 1,
                    data: vec![1, 2, 3],
                });
            }
        }
        assert!(rx.try_recv().is_err());

        // 2. Packet with TTL == 0: Should be dropped
        let ttl_zero_packet = wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            sender_id: 2,
            origin_node: 200u64,
            target_address: UserAddress::new("target_addr_12345"),
            sender_address: UserAddress::new("sender_addr_12345"),
            codec_id: wpapi::protocol::CODEC_OPUS,
            ttl: 0,
            audio_data: vec![4, 5, 6],
        };
        let encoded_zero = ttl_zero_packet.encode();

        if let Ok(wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            origin_node, ttl, ..
        }) = wpapi::protocol::ProtocolPacket::decode(&encoded_zero)
        {
            if origin_node != my_node_id && ttl > 0 {
                let _ = AUDIO_BROADCAST.send(AudioMessage {
                    sender_id: 2,
                    sender_address: None,
                    target_address: UserAddress::new("target_addr_12345"),
                    origin_node,
                    ttl: ttl - 1,
                    data: vec![4, 5, 6],
                });
            }
        }
        assert!(rx.try_recv().is_err());

        // 3. Valid peer packet (origin_node != my_node_id, ttl = 8): Should decrement TTL to 7 and broadcast
        let valid_packet = wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            sender_id: 3,
            origin_node: 300u64,
            target_address: UserAddress::new("target_addr_12345"),
            sender_address: UserAddress::new("sender_addr_12345"),
            codec_id: wpapi::protocol::CODEC_OPUS,
            ttl: 8,
            audio_data: vec![7, 8, 9],
        };
        let encoded_valid = valid_packet.encode();

        if let Ok(wpapi::protocol::ProtocolPacket::PeerTargetedAudio {
            sender_id,
            origin_node,
            target_address,
            sender_address,
            ttl,
            audio_data,
            ..
        }) = wpapi::protocol::ProtocolPacket::decode(&encoded_valid)
        {
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
            }
        }

        let recv_msg = rx.recv().await.expect("Should receive broadcasted message");
        assert_eq!(recv_msg.origin_node, 300u64);
        assert_eq!(recv_msg.ttl, 7); // Decremented from 8 to 7
    }
}
