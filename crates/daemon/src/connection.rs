use crate::broadcast::{AudioMessage, AUDIO_BROADCAST};
use crate::config::CONFIGURATION;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;
use wtransport::endpoint::IncomingSession;

/// Counter for connected clients
static CLIENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Maximum audio message size (1MB should be more than enough for audio buffers)
const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

/// Handle an incoming WebTransport connection
pub async fn handle_connection(incoming: IncomingSession) {
    let result = handle_connection_impl(incoming).await;
    if let Err(e) = result {
        tracing::error!("Connection error: {}", e);
    }
}

async fn handle_connection_impl(
    incoming: IncomingSession,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let session_request = incoming.await?;

    info!("New session request from: {}", session_request.authority());

    let connection = session_request.accept().await?;
    let client_id = CLIENT_COUNT.fetch_add(1, Ordering::SeqCst);
    info!("Client {} connected", client_id);

    // Get allow_echoback setting from configuration
    let allow_echoback = CONFIGURATION
        .get()
        .map(|c| c.allow_echoback)
        .unwrap_or(false);

    // Subscribe to broadcast channel for receiving audio from others
    let mut audio_rx = AUDIO_BROADCAST.subscribe();

    // Open a bidirectional stream for audio
    let (mut send_stream, mut recv_stream) = connection.accept_bi().await?;

    // Spawn task to send audio to this client
    let send_task = tokio::spawn(async move {
        loop {
            match audio_rx.recv().await {
                Ok(audio_msg) => {
                    // Skip if this is our own audio and echoback is disabled
                    if !allow_echoback && audio_msg.sender_id == client_id {
                        continue;
                    }

                    // Send length prefix then data
                    let len = audio_msg.data.len() as u32;
                    if send_stream.write_all(&len.to_le_bytes()).await.is_err() {
                        break;
                    }
                    if send_stream.write_all(&audio_msg.data).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });

    // Receive audio from this client and broadcast to others
    let mut buf = vec![0u8; 65536];
    loop {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        match recv_stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(_) => break,
        }
        let len = u32::from_le_bytes(len_buf) as usize;

        // Validate message size to prevent DoS attacks
        if len > MAX_MESSAGE_SIZE {
            tracing::warn!(
                "Client {} sent oversized message ({} bytes), disconnecting",
                client_id,
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
            Err(_) => break,
        }

        // Broadcast to all clients (filtering happens on receive side)
        let _ = AUDIO_BROADCAST.send(AudioMessage {
            sender_id: client_id,
            data: buf[..len].to_vec(),
        });
    }

    send_task.abort();
    info!("Client {} disconnected", client_id);

    Ok(())
}
