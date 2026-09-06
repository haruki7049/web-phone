//! Audio call module.
//!
//! This module provides the main CLI functionality for establishing and
//! maintaining an audio call connection over WebRTC. It handles:
//!
//! - Establishing WebRTC PeerConnection with wdaemon server
//! - Exchanging SDP offer/answer over HTTP signaling
//! - Capturing microphone audio with cpal
//! - Transmitting audio over WebRTC DataChannel
//! - Receiving and playing back audio through speakers

use crate::config::Configuration;
use anyhow::{Result, anyhow};
use bytes::Bytes;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, StreamConfig};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info};
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

/// Audio buffer for receiving audio data from WebRTC DataChannel.
static AUDIO_BUFFER: LazyLock<Mutex<VecDeque<f32>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

/// Store assigned client ID received from server.
static MY_CLIENT_ID: AtomicU64 = AtomicU64::new(u64::MAX);

/// Start an audio CLI call to the server using WebRTC.
pub async fn start_call(config: &Configuration) -> Result<()> {
    let server_url = match config.server_ip {
        std::net::IpAddr::V4(ip) => format!("http://{}:{}", ip, config.server_port),
        std::net::IpAddr::V6(ip) => format!("http://[{}]:{}", ip, config.server_port),
    };
    info!(
        "Recognized wclient user address (IPv6): {}",
        config.user_address
    );
    info!("Connecting to WebRTC audio server at {}...", server_url);

    let api = APIBuilder::new().build();
    let rtc_config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec![config.stun_server.clone()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let peer_connection = Arc::new(api.new_peer_connection(rtc_config).await?);
    let data_channel = peer_connection.create_data_channel("audio", None).await?;

    let allow_echoback = config.allow_echoback;
    info!(
        "Echo back: {}",
        if allow_echoback {
            "enabled"
        } else {
            "disabled"
        }
    );

    // Channel to send recorded microphone audio bytes from capture callback to DataChannel task
    let (tx_audio, mut rx_audio) = mpsc::channel::<Vec<u8>>(100);

    let dc_open = Arc::clone(&data_channel);
    data_channel.on_open(Box::new(move || {
        let dc_inner = Arc::clone(&dc_open);
        Box::pin(async move {
            info!("WebRTC DataChannel 'audio' successfully opened");

            // Spawn task to send microphone audio over DataChannel
            tokio::spawn(async move {
                while let Some(audio_bytes) = rx_audio.recv().await {
                    let mut packet = Vec::with_capacity(1 + audio_bytes.len());
                    packet.push(0x01); // Audio data message type
                    packet.extend_from_slice(&audio_bytes);

                    if dc_inner.send(&Bytes::from(packet)).await.is_err() {
                        break;
                    }
                }
            });
        })
    }));

    data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
        Box::pin(async move {
            if msg.data.is_empty() {
                return;
            }

            let msg_type = msg.data[0];
            if msg_type == 0x00 && msg.data.len() >= 9 {
                // Client ID assignment message: [0x00, client_id (8 bytes LE)]
                let client_id = u64::from_le_bytes(msg.data[1..9].try_into().unwrap());
                MY_CLIENT_ID.store(client_id, Ordering::SeqCst);
                info!("Assigned client ID from WebRTC server: {}", client_id);
            } else if msg_type == 0x01 && msg.data.len() >= 9 {
                // Broadcast audio message: [0x01, sender_id (8 bytes LE), audio_data...]
                let sender_id = u64::from_le_bytes(msg.data[1..9].try_into().unwrap());
                let my_id = MY_CLIENT_ID.load(Ordering::SeqCst);

                if !allow_echoback && sender_id == my_id {
                    return;
                }

                let samples: Vec<f32> = msg.data[9..]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|chunk| f32::from_le_bytes(*chunk))
                    .collect();

                let mut buffer = AUDIO_BUFFER.lock().unwrap();
                for sample in samples {
                    buffer.push_back(sample);
                }
            }
        })
    }));

    // Generate SDP Offer
    let offer = peer_connection.create_offer(None).await?;
    peer_connection.set_local_description(offer).await?;

    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    let _ = gather_complete.recv().await;

    let local_desc = peer_connection
        .local_description()
        .await
        .ok_or_else(|| anyhow!("Failed to generate local SDP offer"))?;

    // Perform HTTP SDP offer/answer exchange
    let client = reqwest::Client::new();
    let sdp_endpoint = format!("{}/sdp", server_url);

    info!("Sending SDP offer to {}...", sdp_endpoint);
    let resp = client.post(&sdp_endpoint).json(&local_desc).send().await?;

    if !resp.status().is_success() {
        return Err(anyhow!("Server returned error: {}", resp.status()));
    }

    let answer: RTCSessionDescription = resp.json().await?;
    peer_connection.set_remote_description(answer).await?;

    info!("Connected to audio server via WebRTC!");
    info!("Press Ctrl-C to disconnect");

    // Set up cpal audio host and devices
    let host = cpal::default_host();

    let input_device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No input device available"))?;
    info!("Using input device: {}", input_device.name()?);

    let output_device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No output device available"))?;
    info!("Using output device: {}", output_device.name()?);

    let sample_rate = SampleRate(config.sample_rate);
    let channels = config.channels;
    let stream_config = StreamConfig {
        channels,
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    let input_stream = input_device.build_input_stream(
        &stream_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let bytes: Vec<u8> = data
                .iter()
                .flat_map(|&sample| sample.to_le_bytes())
                .collect();
            let _ = tx_audio.blocking_send(bytes);
        },
        |err| error!("Input stream error: {}", err),
        None,
    )?;

    let output_stream = output_device.build_output_stream(
        &stream_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut buffer = AUDIO_BUFFER.lock().unwrap();
            for sample in data.iter_mut() {
                *sample = buffer.pop_front().unwrap_or(0.0);
            }
        },
        |err| error!("Output stream error: {}", err),
        None,
    )?;

    input_stream.play()?;
    output_stream.play()?;

    info!("Audio streams started. Speaking now will transmit audio over WebRTC.");

    // Keep call running until process is interrupted
    tokio::signal::ctrl_c().await?;
    info!("Call ended by user signal");

    Ok(())
}
