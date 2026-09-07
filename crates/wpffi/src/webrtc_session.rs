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

    let dc_clone = Arc::clone(&data_channel);
    let target_opt = target_address;
    data_channel.on_open(Box::new(move || {
        let dc_inner = Arc::clone(&dc_clone);
        Box::pin(async move {
            info!("WebRTC DataChannel 'audio' successfully opened");

            tokio::spawn(async move {
                while let Some(audio_bytes) = rx_audio.recv().await {
                    if let Some(ref target) = target_opt {
                        let packet = ProtocolPacket::ClientTargetedAudio {
                            target_address: target.clone(),
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

    data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
        let dc_inner = Arc::clone(&dc_msg);
        let my_client_id = Arc::clone(&my_client_id);
        let audio_buffer = Arc::clone(&audio_buffer);
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
                }
                ProtocolPacket::CallRequest { caller_address, .. } => {
                    let caller_bytes = caller_address.to_bytes();
                    if auto_accept {
                        info!(
                            "Auto-accepting incoming call request from {} (Short ID: {})",
                            caller_address,
                            caller_address.short_id()
                        );
                        let resp = ProtocolPacket::CallAcceptResponse { caller_address };
                        let _ = dc_inner.send(&Bytes::from(resp.encode())).await;
                    } else {
                        let dc_reply = Arc::clone(&dc_inner);
                        let caller_addr_clone = caller_address.clone();

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
                    std::process::exit(1);
                }
                ProtocolPacket::CallAcceptedNotification { target_address } => {
                    info!("============================================================");
                    info!(
                        " Call connection accepted by target user ({})!",
                        target_address.short_id()
                    );
                    info!(" Call connected.");
                    info!("============================================================");
                }
                ProtocolPacket::ConnectionError { target_address } => {
                    error!("============================================================");
                    error!(
                        " Call Connection Error: Target user ({}) is currently in another call.",
                        target_address.short_id()
                    );
                    error!(
                        " Calls are restricted to 1-to-1 only (Maximum 2 participants allowed)."
                    );
                    error!(" Connection rejected.");
                    error!("============================================================");
                    std::process::exit(1);
                }
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
) -> Result<()> {
    let offer = peer_connection.create_offer(None).await?;
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection.set_local_description(offer).await?;
    let _ = gather_complete.recv().await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or_else(|| anyhow!("Failed to get local SDP description"))?;

    let server_url = match config.server_ip {
        std::net::IpAddr::V4(ip) => format!("http://{}:{}", ip, config.server_port),
        std::net::IpAddr::V6(ip) => format!("http://[{}]:{}", ip, config.server_port),
    };

    let sdp_endpoint = format!("{}/sdp", server_url);
    info!("Sending SDP offer to {}...", sdp_endpoint);
    let client = reqwest::Client::new();
    let resp = client.post(&sdp_endpoint).json(&local_desc).send().await?;

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
