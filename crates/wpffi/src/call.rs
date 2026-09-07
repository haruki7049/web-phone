//! Audio call module.
//!
//! This module orchestrates WebRTC communication sessions and CPAL Audio Engines.

use crate::address::UserAddress;
use crate::audio::AudioEngine;
use crate::config::Configuration;
use crate::session::ClientSession;
use crate::webrtc_session::{create_peer_connection, perform_sdp_handshake, setup_data_channel};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

/// Start an audio CLI call to the server using WebRTC.
pub async fn start_call(config: &Configuration, target_address: Option<UserAddress>) -> Result<()> {
    start_call_with_cancel(config, target_address, None).await
}

/// Start an audio CLI call with optional cancellation receiver.
pub async fn start_call_with_cancel(
    config: &Configuration,
    target_address: Option<UserAddress>,
    cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    let session = ClientSession::new();
    start_call_with_session(config, target_address, cancel_rx, &session).await
}

/// Start an audio CLI call with a specific `ClientSession` and optional cancellation receiver.
pub async fn start_call_with_session(
    config: &Configuration,
    target_address: Option<UserAddress>,
    mut cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    session: &ClientSession,
) -> Result<()> {
    if let Some(ref target) = target_address {
        info!(
            "Targeting direct 1-to-1 call to wpclient user ID: {}",
            target
        );
    } else {
        info!("No target address specified. Operating in incoming call standby mode...");
    }

    // 1. Create WebRTC PeerConnection
    let peer_connection = create_peer_connection(config).await?;

    // 2. Setup DataChannel & audio byte sender channel
    let (tx_audio, rx_audio) = mpsc::channel::<Vec<u8>>(100);
    let _data_channel =
        setup_data_channel(&peer_connection, target_address.clone(), rx_audio, config, session).await?;

    // 3. Perform SDP Offer / Answer exchange with server
    perform_sdp_handshake(&peer_connection, config).await?;

    // 4. Start CPAL Audio Engine (Microphone & Speaker Streams)
    let _audio_engine = AudioEngine::start(config, tx_audio, Arc::clone(&session.audio_buffer))?;

    // 5. Keep call running until interrupted or cancelled
    if let Some(rx) = cancel_rx.as_mut() {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Call ended by user signal");
            }
            _ = rx => {
                info!("Call ended by cancel signal");
            }
        }
    } else {
        tokio::signal::ctrl_c().await?;
        info!("Call ended by user signal");
    }

    // Send WPIP-07 CallHangup (0x0B) notification if targeting a specific client
    if let Some(ref target) = target_address {
        let hangup_packet = crate::protocol::ProtocolPacket::CallHangup {
            target_address: target.clone(),
        };
        let _ = _data_channel
            .send(&bytes::Bytes::from(hangup_packet.encode()))
            .await;
    }

    let _ = peer_connection.close().await;
    Ok(())
}
