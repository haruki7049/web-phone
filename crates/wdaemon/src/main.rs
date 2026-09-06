//! WebRTC audio daemon, STUN/TURN server, and peer mesh entry point.

use axum::{routing::post, Router};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{error, info};
use wdaemon::{
    connection::handle_sdp_offer, peer::handle_peer_sdp, peer::connect_to_peer, stun::run_stun_server,
    CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH,
};

/// Main entry point for the audio server daemon.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    color_eyre::install().expect("Failed to install color_eyre panic handler");
    tracing_subscriber::fmt::init();

    let args: CLIArgs = CLIArgs::parse();

    let mut loaded_config: Configuration = confy::load_path(&args.config_path).unwrap_or_else(|_| {
        info!("Running wdaemon with default Configuration...");
        Configuration::default()
    });

    if let Some(port) = args.port {
        loaded_config.port = port;
    }
    if let Some(stun_port) = args.stun_port {
        loaded_config.stun_port = stun_port;
    }
    if !args.peer.is_empty() {
        loaded_config.peers.extend(args.peer);
    }

    CONFIGURATION
        .set(loaded_config)
        .unwrap();

    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or("Failed to get Configuration from CONFIGURATION")?;

    // Spawn STUN/TURN UDP server if enabled
    if config.turn_enabled {
        let stun_addr: SocketAddr = format!("{}:{}", config.ip, config.stun_port).parse()?;
        tokio::spawn(async move {
            if let Err(e) = run_stun_server(stun_addr).await {
                error!("STUN/TURN server error: {}", e);
            }
        });
    }

    // Connect to peer wdaemon instances if specified
    for peer_url in config.peers.clone() {
        let url = peer_url.clone();
        tokio::spawn(async move {
            // Small delay to allow peer servers to start if launched simultaneously
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            if let Err(e) = connect_to_peer(url.clone()).await {
                error!("Failed to connect to peer wdaemon at {}: {}", url, e);
            }
        });
    }

    let address: SocketAddr = format!("{}:{}", config.ip, config.port).parse()?;

    let app = Router::new()
        .route("/sdp", post(handle_sdp_offer))
        .route("/peer/sdp", post(handle_peer_sdp));

    let listener = tokio::net::TcpListener::bind(address).await?;
    info!("WebRTC audio daemon node {} running on http://{}", config.node_id, &address);
    if config.turn_enabled {
        info!("STUN/TURN server running on UDP {}:{}", config.ip, config.stun_port);
    }
    info!("Waiting for wclient audio calls & peer daemon mesh connections...");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Command-line arguments for the audio server daemon.
#[derive(Parser)]
struct CLIArgs {
    /// Path to the configuration file.
    #[arg(short, long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,

    /// HTTP/WebRTC signaling port override.
    #[arg(short, long)]
    port: Option<u16>,

    /// STUN/TURN UDP port override.
    #[arg(short, long)]
    stun_port: Option<u16>,

    /// Peer wdaemon URLs to connect to for mesh interconnection.
    #[arg(long)]
    peer: Vec<String>,
}
