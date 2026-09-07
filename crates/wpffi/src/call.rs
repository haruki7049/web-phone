//! Audio call module.
//!
//! This module orchestrates WebRTC communication sessions and CPAL Audio Engines.

use crate::address::UserAddress;
use crate::audio::AudioEngine;
use crate::config::Configuration;
use crate::webrtc_session::{create_peer_connection, perform_sdp_handshake, setup_data_channel};
use anyhow::Result;
use std::collections::VecDeque;
use std::sync::atomic::AtomicU64;
use std::sync::{LazyLock, Mutex};
use tokio::sync::mpsc;
use tracing::info;

/// Audio buffer for receiving audio data from WebRTC DataChannel.
pub static AUDIO_BUFFER: LazyLock<Mutex<VecDeque<f32>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

/// Store assigned client ID received from server.
pub static MY_CLIENT_ID: AtomicU64 = AtomicU64::new(u64::MAX);

/// Start an audio CLI call to the server using WebRTC.
pub async fn start_call(config: &Configuration, target_address: Option<UserAddress>) -> Result<()> {
    start_call_with_cancel(config, target_address, None).await
}

/// Start an audio CLI call with optional cancellation receiver.
pub async fn start_call_with_cancel(
    config: &Configuration,
    target_address: Option<UserAddress>,
    mut cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    if let Some(ref target) = target_address {
        info!("Targeting direct 1-to-1 call to wpclient user ID: {}", target);
    } else {
        info!("No target address specified. Operating in incoming call standby mode...");
    }

    // 1. Create WebRTC PeerConnection
    let peer_connection = create_peer_connection(config).await?;

    // 2. Setup DataChannel & audio byte sender channel
    let (tx_audio, rx_audio) = mpsc::channel::<Vec<u8>>(100);
    let _data_channel = setup_data_channel(
        &peer_connection,
        target_address,
        rx_audio,
        config,
        &MY_CLIENT_ID,
        &AUDIO_BUFFER,
    )
    .await?;

    // 3. Perform SDP Offer / Answer exchange with server
    perform_sdp_handshake(&peer_connection, config).await?;

    // 4. Start CPAL Audio Engine (Microphone & Speaker Streams)
    let _audio_engine = AudioEngine::start(config, tx_audio, &AUDIO_BUFFER)?;

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

    let _ = peer_connection.close().await;
    Ok(())
}
