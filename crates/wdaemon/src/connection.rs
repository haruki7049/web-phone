//! Connection handling module.
//!
//! This module provides the logic for handling WebRTC SDP offer requests,
//! managing client PeerConnections, and routing audio between connected WebRTC clients.

use crate::broadcast::{AUDIO_BROADCAST, AudioMessage};
use crate::config::CONFIGURATION;
use axum::{extract::Json, http::StatusCode};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tracing::{error, info, warn};
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

/// Counter for connected clients.
static CLIENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Active WebRTC peer connections.
static PEER_CONNECTIONS: LazyLock<Mutex<HashMap<u64, Arc<RTCPeerConnection>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Maximum audio message size in bytes (1MB).
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Handle an SDP offer from a WebRTC client.
pub async fn handle_sdp_offer(
    Json(offer): Json<RTCSessionDescription>,
) -> Result<Json<RTCSessionDescription>, (StatusCode, String)> {
    let daemon_config = CONFIGURATION.get().cloned().unwrap_or_default();
    let my_node_id = daemon_config.node_id;

    let api = APIBuilder::new().build();
    let config = RTCConfiguration::default();

    let peer_connection = Arc::new(
        api.new_peer_connection(config)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create PeerConnection: {}", e)))?,
    );

    let client_id = CLIENT_COUNT.fetch_add(1, Ordering::SeqCst);
    info!("Client {} initiating WebRTC connection", client_id);

    PEER_CONNECTIONS
        .lock()
        .unwrap()
        .insert(client_id, Arc::clone(&peer_connection));

    peer_connection.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
        info!("Client {} PeerConnection state: {}", client_id, state);
        if state == RTCPeerConnectionState::Failed
            || state == RTCPeerConnectionState::Closed
            || state == RTCPeerConnectionState::Disconnected
        {
            info!("Client {} disconnected", client_id);
            PEER_CONNECTIONS.lock().unwrap().remove(&client_id);
        }
        Box::pin(async move {})
    }));

    peer_connection.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        let dc_label = dc.label().to_string();
        info!("Client {} created DataChannel: {}", client_id, dc_label);

        let dc_open = Arc::clone(&dc);

        dc.on_open(Box::new(move || {
            let dc_inner = Arc::clone(&dc_open);
            let mut audio_rx = AUDIO_BROADCAST.subscribe();

            Box::pin(async move {
                info!("Client {} DataChannel opened", client_id);

                // Send client ID assignment message: [0x00, client_id (8 bytes LE)]
                let mut init_msg = Vec::with_capacity(9);
                init_msg.push(0x00);
                init_msg.extend_from_slice(&client_id.to_le_bytes());

                if let Err(e) = dc_inner.send(&Bytes::from(init_msg)).await {
                    error!("Failed to send client ID to client {}: {}", client_id, e);
                    return;
                }

                // Forward broadcast audio to this client
                tokio::spawn(async move {
                    loop {
                        match audio_rx.recv().await {
                            Ok(audio_msg) => {
                                // Packet: [0x01, sender_id (8 bytes LE), audio_data...]
                                let mut packet = Vec::with_capacity(9 + audio_msg.data.len());
                                packet.push(0x01);
                                packet.extend_from_slice(&audio_msg.sender_id.to_le_bytes());
                                packet.extend_from_slice(&audio_msg.data);

                                if dc_inner.send(&Bytes::from(packet)).await.is_err() {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        }
                    }
                });
            })
        }));

        dc.on_message(Box::new(move |msg: DataChannelMessage| {
            Box::pin(async move {
                if msg.data.is_empty() {
                    return;
                }

                let msg_type = msg.data[0];
                if msg_type == 0x01 {
                    // Audio message from client: [0x01, audio_data...]
                    let payload = msg.data[1..].to_vec();
                    if payload.len() > MAX_MESSAGE_SIZE {
                        warn!(
                            "Client {} sent oversized audio packet ({} bytes), ignoring",
                            client_id,
                            payload.len()
                        );
                        return;
                    }

                    let _ = AUDIO_BROADCAST.send(AudioMessage {
                        sender_id: client_id,
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

    let answer = peer_connection
        .create_answer(None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create answer: {}", e)))?;

    peer_connection
        .set_local_description(answer)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to set local description: {}", e)))?;

    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    let _ = gather_complete.recv().await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "No local description available".to_string()))?;

    Ok(Json(local_desc))
}
