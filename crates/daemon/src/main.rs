use clap::Parser;
use daemon::{AUDIO_BROADCAST, CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;
use wtransport::endpoint::IncomingSession;
use wtransport::{Endpoint, Identity, ServerConfig};

static CLIENT_COUNT: AtomicU64 = AtomicU64::new(0);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args: CLIArgs = CLIArgs::parse();
    CONFIGURATION
        .set(confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running web-phone-daemon with default Configuration...");
            Configuration::default()
        }))
        .unwrap();
    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or("Failed to get Configuration from CONFIGURATION")?;

    let address: SocketAddr = format!("{}:{}", &config.ip, &config.port).parse()?;

    // Generate self-signed certificate for WebTransport
    let identity = Identity::self_signed(["localhost", "127.0.0.1", &config.ip.to_string()])?;

    let server_config = ServerConfig::builder()
        .with_bind_address(address)
        .with_identity(identity)
        .build();

    let endpoint = Endpoint::server(server_config)?;

    info!(
        "WebTransport audio server running on https://{}",
        &address
    );
    info!("Use Ctrl-C to stop this program");
    info!("Waiting for audio clients to connect...");

    // Accept incoming connections
    loop {
        let incoming = endpoint.accept().await;
        tokio::spawn(handle_connection(incoming));
    }
}

async fn handle_connection(incoming: IncomingSession) {
    let result = handle_connection_impl(incoming).await;
    if let Err(e) = result {
        tracing::error!("Connection error: {}", e);
    }
}

async fn handle_connection_impl(
    incoming: IncomingSession,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let session_request = incoming.await?;

    info!(
        "New session request from: {}",
        session_request.authority()
    );

    let connection = session_request.accept().await?;
    let client_id = CLIENT_COUNT.fetch_add(1, Ordering::SeqCst);
    info!("Client {} connected", client_id);

    // Subscribe to broadcast channel for receiving audio from others
    let mut audio_rx = AUDIO_BROADCAST.subscribe();

    // Open a bidirectional stream for audio
    let (mut send_stream, mut recv_stream) = connection.accept_bi().await?;

    // Spawn task to send audio to this client
    let send_task = tokio::spawn(async move {
        loop {
            match audio_rx.recv().await {
                Ok(audio_data) => {
                    // Send length prefix then data
                    let len = audio_data.len() as u32;
                    if send_stream.write_all(&len.to_le_bytes()).await.is_err() {
                        break;
                    }
                    if send_stream.write_all(&audio_data).await.is_err() {
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

        if len > buf.len() {
            buf.resize(len, 0);
        }

        // Read audio data
        match recv_stream.read_exact(&mut buf[..len]).await {
            Ok(_) => {}
            Err(_) => break,
        }

        // Broadcast to all other clients
        let _ = AUDIO_BROADCAST.send(buf[..len].to_vec());
    }

    send_task.abort();
    info!("Client {} disconnected", client_id);

    Ok(())
}

#[derive(Parser)]
struct CLIArgs {
    #[arg(short, long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,
}
