//! Audio call module.
//!
//! This module provides the main functionality for establishing and
//! maintaining an audio call connection over WebTransport. It handles:
//!
//! - Connecting to the WebTransport server
//! - Capturing audio from the microphone
//! - Transmitting audio to the server
//! - Receiving audio from other clients
//! - Playing received audio through speakers

use crate::config::{Configuration, TlsVerifyMode};
use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, StreamConfig};
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use wtransport::tls::Sha256Digest;
use wtransport::tls::Sha256DigestFmt;
use wtransport::{ClientConfig, Endpoint};

/// Audio buffer for receiving audio data from WebTransport.
///
/// This FIFO queue stores incoming audio samples until they are
/// consumed by the audio output stream for playback.
static AUDIO_BUFFER: LazyLock<Mutex<VecDeque<f32>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

/// Maximum audio message size in bytes (1MB).
///
/// This limit prevents excessive memory allocation from malicious
/// or malformed messages from the server.
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Start an audio call to the server.
///
/// This function establishes a WebTransport connection to the audio
/// server and begins bidirectional audio streaming. It:
///
/// 1. Connects to the server using WebTransport/HTTP3/QUIC
/// 2. Receives a unique client ID from the server
/// 3. Sets up microphone capture and speaker playback
/// 4. Transmits captured audio to the server
/// 5. Receives and plays audio from other connected clients
///
/// # Arguments
///
/// * `config` - Client configuration containing server address and audio settings
///
/// # Returns
///
/// Returns `Ok(())` when the call ends normally, or an error if
/// connection or audio setup fails.
///
/// # TLS Verification
///
/// The TLS certificate verification behavior is determined by the
/// `tls_verify` configuration option:
///
/// - `TlsVerifyMode::Skip` - Skips certificate verification (development only)
/// - `TlsVerifyMode::Native` - Uses system's native certificate store (production)
/// - `TlsVerifyMode::CertificateHash` - Pins specific certificate hashes
pub async fn start_call(config: &Configuration) -> Result<()> {
    let server_url = format!("https://{}:{}", config.server_ip, config.server_port);
    info!("Connecting to audio server at {}...", server_url);

    // Build client configuration based on TLS verification mode
    let client_config = build_client_config(&config.tls_verify)?;

    let endpoint = Endpoint::client(client_config)?;
    let connection = endpoint.connect(&server_url).await?;

    info!("Connected to audio server via WebTransport");
    info!("Press Ctrl-C to disconnect");

    // Open bidirectional stream for audio
    let (mut send_stream, mut recv_stream) = connection.open_bi().await?.await?;

    // Receive our client_id from server
    let mut client_id_buf = [0u8; 8];
    recv_stream.read_exact(&mut client_id_buf).await?;
    let my_client_id = u64::from_le_bytes(client_id_buf);
    info!("Assigned client ID: {}", my_client_id);

    // Get allow_echoback setting from config
    let allow_echoback = config.allow_echoback;
    info!(
        "Echo back: {}",
        if allow_echoback {
            "enabled"
        } else {
            "disabled"
        }
    );

    // Create channel for sending audio from capture thread
    let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(100);

    // Set up audio host
    let host = cpal::default_host();

    // Set up input (microphone)
    let input_device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No input device available"))?;
    info!("Using input device: {}", input_device.name()?);

    // Set up output (speakers)
    let output_device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No output device available"))?;
    info!("Using output device: {}", output_device.name()?);

    // Configure audio format - use a common format
    let sample_rate = SampleRate(config.sample_rate);
    let channels = config.channels;

    let stream_config = StreamConfig {
        channels,
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    // Create input stream (capture from microphone)
    let input_stream = input_device.build_input_stream(
        &stream_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // Convert f32 samples to bytes for transmission
            let bytes: Vec<u8> = data
                .iter()
                .flat_map(|&sample| sample.to_le_bytes())
                .collect();

            // Send audio data through channel
            let _ = audio_tx.blocking_send(bytes);
        },
        |err| error!("Input stream error: {}", err),
        None,
    )?;

    // Create output stream (play to speakers)
    let output_stream = output_device.build_output_stream(
        &stream_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut buffer = AUDIO_BUFFER.lock().unwrap();

            // Fill output buffer with received audio (FIFO) or silence
            for sample in data.iter_mut() {
                *sample = buffer.pop_front().unwrap_or(0.0);
            }
        },
        |err| error!("Output stream error: {}", err),
        None,
    )?;

    // Start streams
    input_stream.play()?;
    output_stream.play()?;

    info!("Audio streams started. Speaking now will transmit audio.");

    // Task to send audio to server
    let send_task = tokio::spawn(async move {
        while let Some(audio_data) = audio_rx.recv().await {
            // Send length prefix then data
            let len = audio_data.len() as u32;
            if send_stream.write_all(&len.to_le_bytes()).await.is_err() {
                break;
            }
            if send_stream.write_all(&audio_data).await.is_err() {
                break;
            }
        }
    });

    // Receive audio from server
    let mut buf = vec![0u8; 65536];
    loop {
        // Read sender_id (8 bytes)
        let mut sender_id_buf = [0u8; 8];
        match recv_stream.read_exact(&mut sender_id_buf).await {
            Ok(_) => {}
            Err(e) => {
                error!("Connection closed: {}", e);
                break;
            }
        }
        let sender_id = u64::from_le_bytes(sender_id_buf);

        // Read length prefix
        let mut len_buf = [0u8; 4];
        match recv_stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) => {
                error!("Connection closed: {}", e);
                break;
            }
        }
        let len = u32::from_le_bytes(len_buf) as usize;

        // Validate message size to prevent excessive memory allocation
        if len > MAX_MESSAGE_SIZE {
            error!(
                "Server sent oversized message ({} bytes), disconnecting",
                len
            );
            break;
        }

        if len > buf.len() {
            buf.resize(len, 0);
        }

        // Read audio data
        match recv_stream.read_exact(&mut buf[..len]).await {
            Ok(_) => {}
            Err(e) => {
                error!("Failed to read audio: {}", e);
                break;
            }
        }

        // Skip our own audio if echoback is disabled
        if !allow_echoback && sender_id == my_client_id {
            continue;
        }

        // Convert bytes back to f32 samples
        let samples: Vec<f32> = buf[..len]
            .chunks_exact(4)
            .map(|chunk| {
                let bytes: [u8; 4] = chunk.try_into().unwrap();
                f32::from_le_bytes(bytes)
            })
            .collect();

        // Add to playback buffer (FIFO order for proper audio playback)
        let mut buffer = AUDIO_BUFFER.lock().unwrap();
        for sample in samples {
            buffer.push_back(sample);
        }
    }

    send_task.abort();
    info!("Disconnected from server");

    Ok(())
}

/// Build WebTransport client configuration based on TLS verification mode.
///
/// This function creates the appropriate [`ClientConfig`] based on the
/// specified [`TlsVerifyMode`]:
///
/// - `Skip`: No certificate verification (development only)
/// - `Native`: Uses system's native certificate store
/// - `CertificateHash`: Pins specific certificate SHA-256 hashes
///
/// # Arguments
///
/// * `tls_verify` - The TLS verification mode to use
///
/// # Returns
///
/// Returns a configured [`ClientConfig`] or an error if configuration fails.
fn build_client_config(tls_verify: &TlsVerifyMode) -> Result<ClientConfig> {
    let client_config = match tls_verify {
        TlsVerifyMode::Skip => {
            warn!("TLS certificate verification is disabled - use only for development!");
            ClientConfig::builder()
                .with_bind_default()
                .with_no_cert_validation()
                .build()
        }
        TlsVerifyMode::Native => {
            info!("Using native certificate store for TLS verification");
            ClientConfig::builder()
                .with_bind_default()
                .with_native_certs()
                .build()
        }
        TlsVerifyMode::CertificateHash { hashes } => {
            info!(
                "Using certificate hash verification with {} hash(es)",
                hashes.len()
            );
            let digests: Vec<Sha256Digest> = hashes
                .iter()
                .map(|h| {
                    // Try parsing as dotted hex format (e.g., "ab:cd:ef:...")
                    Sha256Digest::from_str_fmt(h, Sha256DigestFmt::DottedHex)
                        .or_else(|_| {
                            // Fallback to bytes array format (e.g., "[0xab, 0xcd, ...]")
                            Sha256Digest::from_str_fmt(h, Sha256DigestFmt::BytesArray)
                        })
                        .map_err(|_| anyhow!("Invalid certificate hash format: {}", h))
                })
                .collect::<Result<Vec<_>>>()?;

            ClientConfig::builder()
                .with_bind_default()
                .with_server_certificate_hashes(digests)
                .build()
        }
    };

    Ok(client_config)
}
