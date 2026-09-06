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

use crate::address::UserAddress;
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
/// Optional `target_address` specifies the target registered wclient temporary SHA-256 user ID for a 1-to-1 call.
/// If `target_address` is None, the client operates in standby mode (ready to receive calls to its own assigned ID).
pub async fn start_call(config: &Configuration, target_address: Option<UserAddress>) -> Result<()> {
    let server_url = match config.server_ip {
        std::net::IpAddr::V4(ip) => format!("http://{}:{}", ip, config.server_port),
        std::net::IpAddr::V6(ip) => format!("http://[{}]:{}", ip, config.server_port),
    };
    if let Some(ref target) = target_address {
        info!(
            "Targeting direct 1-to-1 call to wclient user ID: {}",
            target
        );
    } else {
        info!("No target address specified. Operating in incoming call standby mode...");
    }
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
    let target_addr_send = target_address.clone();
    data_channel.on_open(Box::new(move || {
        let dc_inner = Arc::clone(&dc_open);
        let target_opt = target_addr_send.clone();
        Box::pin(async move {
            info!("WebRTC DataChannel 'audio' successfully opened");

            // Spawn task to send microphone audio over DataChannel if target address is provided
            tokio::spawn(async move {
                while let Some(audio_bytes) = rx_audio.recv().await {
                    if let Some(ref target) = target_opt {
                        // Targeted 1-to-1 call packet: [0x03, target_address (32 bytes SHA256), audio_bytes...]
                        let mut packet = Vec::with_capacity(33 + audio_bytes.len());
                        packet.push(0x03);
                        packet.extend_from_slice(&target.to_bytes());
                        packet.extend_from_slice(&audio_bytes);

                        if dc_inner.send(&Bytes::from(packet)).await.is_err() {
                            break;
                        }
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
                // Client assignment message: [0x00, client_id (8 bytes LE), user_address (32 bytes SHA256)]
                let client_id = u64::from_le_bytes(msg.data[1..9].try_into().unwrap());
                MY_CLIENT_ID.store(client_id, Ordering::SeqCst);

                if msg.data.len() >= 41 {
                    let addr_bytes: [u8; 32] = msg.data[9..41].try_into().unwrap();
                    let registered_addr = UserAddress::from_bytes(addr_bytes);
                    info!("============================================================");
                    info!(" Assigned Temporary User ID (SHA-256): {}", registered_addr);
                    info!(" Short ID: {}", registered_addr.short_id());
                    info!(" Client ID: {}", client_id);
                    info!("============================================================");
                } else {
                    info!("Assigned client ID from WebRTC server: {}", client_id);
                }
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
            } else if msg_type == 0x03 && msg.data.len() >= 73 {
                // Targeted audio message: [0x03, target_address (32b), sender_id (8b), sender_address (32b), audio_data...]
                let sender_id = u64::from_le_bytes(msg.data[33..41].try_into().unwrap());
                let sender_bytes: [u8; 32] = msg.data[41..73].try_into().unwrap();
                let sender_addr = UserAddress::from_bytes(sender_bytes);
                let my_id = MY_CLIENT_ID.load(Ordering::SeqCst);

                if !allow_echoback && sender_id == my_id {
                    return;
                }

                tracing::trace!("Received audio frame from {}", sender_addr.short_id());

                let samples: Vec<f32> = msg.data[73..]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|chunk| f32::from_le_bytes(*chunk).clamp(-1.0, 1.0))
                    .collect();

                let mut buffer = AUDIO_BUFFER.lock().unwrap();
                for sample in samples {
                    buffer.push_back(sample);
                }
            } else if msg_type == 0xFF {
                // Connection rejection message: [0xFF, target_address (32b)]
                let target_str = if msg.data.len() >= 33 {
                    let addr_bytes: [u8; 32] = msg.data[1..33].try_into().unwrap();
                    UserAddress::from_bytes(addr_bytes).short_id().to_string()
                } else {
                    "target".to_string()
                };
                error!("============================================================");
                error!(
                    " Call Connection Error: Target user ({}) is currently in another call.",
                    target_str
                );
                error!(" Calls are restricted to 1-to-1 only (Maximum 2 participants allowed).");
                error!(" Connection rejected.");
                error!("============================================================");
                std::process::exit(1);
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

    let target_input_config = StreamConfig {
        channels: config.channels,
        sample_rate: SampleRate(config.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let net_sample_rate = config.sample_rate;

    let (input_stream, _actual_input_rate, _actual_input_channels) = {
        let tx_audio_clone = tx_audio.clone();
        let build = |cfg: &StreamConfig, rate: u32, ch: u16| {
            let mut resampler = crate::resample::Resampler::new(rate, net_sample_rate);
            let tx = tx_audio_clone.clone();
            input_device.build_input_stream(
                cfg,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let channels = ch as usize;
                    let num_frames = data.len() / channels;
                    if num_frames == 0 {
                        return;
                    }
                    let mut mono_samples = Vec::with_capacity(num_frames);
                    for f in 0..num_frames {
                        let sum: f32 = (0..channels).map(|c| data[f * channels + c]).sum();
                        mono_samples.push(sum / channels as f32);
                    }
                    let resampled = resampler.process(&mono_samples);
                    let bytes: Vec<u8> = resampled
                        .iter()
                        .flat_map(|&sample| sample.to_le_bytes())
                        .collect();
                    let _ = tx.blocking_send(bytes);
                },
                |err| error!("Input stream error: {}", err),
                None,
            )
        };

        match build(&target_input_config, config.sample_rate, config.channels) {
            Ok(stream) => (stream, config.sample_rate, config.channels),
            Err(err) => {
                info!(
                    "Requested input stream config ({:?}) not supported ({}), falling back to device default config...",
                    target_input_config, err
                );
                let def_cfg = input_device.default_input_config()?;
                let def_stream_config: StreamConfig = def_cfg.config();
                let rate = def_stream_config.sample_rate.0;
                let ch = def_stream_config.channels;
                info!("Using input device default config: {} Hz, {} channels", rate, ch);
                let stream = build(&def_stream_config, rate, ch)?;
                (stream, rate, ch)
            }
        }
    };

    let target_output_config = StreamConfig {
        channels: config.channels,
        sample_rate: SampleRate(config.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let (output_stream, _actual_output_rate, _actual_output_channels) = {
        let build = |cfg: &StreamConfig, rate: u32, ch: u16| {
            let mut output_resampler = crate::resample::Resampler::new(net_sample_rate, rate);
            let mut leftover: Vec<f32> = Vec::new();
            let channels = ch as usize;

            output_device.build_output_stream(
                cfg,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let frames_needed = data.len() / channels;
                    if frames_needed == 0 {
                        return;
                    }

                    if leftover.len() < frames_needed {
                        let needed_resampled = frames_needed - leftover.len();
                        let net_needed = ((needed_resampled as f64 * (net_sample_rate as f64 / rate as f64)).ceil() as usize) + 4;
                        let mut net_samples = Vec::with_capacity(net_needed);
                        {
                            let mut buffer = AUDIO_BUFFER.lock().unwrap();
                            let drain_count = net_needed.min(buffer.len());
                            for _ in 0..drain_count {
                                if let Some(s) = buffer.pop_front() {
                                    net_samples.push(s);
                                }
                            }
                        }
                        let mut resampled = output_resampler.process(&net_samples);
                        leftover.append(&mut resampled);
                    }

                    for f in 0..frames_needed {
                        let sample = leftover.get(f).copied().unwrap_or(0.0);
                        for c in 0..channels {
                            data[f * channels + c] = sample;
                        }
                    }

                    let drain_len = frames_needed.min(leftover.len());
                    leftover.drain(0..drain_len);
                },
                |err| error!("Output stream error: {}", err),
                None,
            )
        };

        match build(&target_output_config, config.sample_rate, config.channels) {
            Ok(stream) => (stream, config.sample_rate, config.channels),
            Err(err) => {
                info!(
                    "Requested output stream config ({:?}) not supported ({}), falling back to device default config...",
                    target_output_config, err
                );
                let def_cfg = output_device.default_output_config()?;
                let def_stream_config: StreamConfig = def_cfg.config();
                let rate = def_stream_config.sample_rate.0;
                let ch = def_stream_config.channels;
                info!("Using output device default config: {} Hz, {} channels", rate, ch);
                let stream = build(&def_stream_config, rate, ch)?;
                (stream, rate, ch)
            }
        }
    };

    input_stream.play()?;
    output_stream.play()?;

    info!("Audio streams started. Speaking now will transmit audio over WebRTC.");

    // Keep call running until process is interrupted
    tokio::signal::ctrl_c().await?;
    info!("Call ended by user signal");

    Ok(())
}
