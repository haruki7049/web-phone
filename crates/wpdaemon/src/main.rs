//! WebRTC audio daemon, STUN/TURN server, and peer mesh entry point.

use axum::{Router, middleware, routing::post};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{error, info};
use wpdaemon::{
    CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH, peer::connect_to_peer,
    rate_limit_middleware, stun::run_stun_server,
};

/// Main entry point for the audio server daemon.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    color_eyre::install().expect("Failed to install color_eyre panic handler");
    tracing_subscriber::fmt::init();

    let args: CLIArgs = CLIArgs::parse();

    let mut loaded_config: Configuration =
        confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running wpdaemon with default Configuration...");
            Configuration::default()
        });

    if let Some(port) = args.port {
        loaded_config.port = port;
    }
    if let Some(stun_port) = args.stun_port {
        loaded_config.stun_port = stun_port;
    }
    if let Some(node_id) = args.node_id {
        loaded_config.node_id = node_id;
    } else if loaded_config.node_id == 0 {
        loaded_config.node_id = wpdaemon::config::generate_node_id();
    }
    if let Some(max_conn) = args.max_connections {
        loaded_config.max_connections = max_conn;
    }
    if !args.peer.is_empty() {
        loaded_config.peers.extend(args.peer);
    }

    CONFIGURATION.set(loaded_config).unwrap();

    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or("Failed to get Configuration from CONFIGURATION")?;

    // Spawn STUN/TURN UDP server if enabled
    if config.turn_enabled {
        let stun_addr = SocketAddr::new(config.ip, config.stun_port);
        info!(
            "STUN/TURN HMAC authentication active (node ID: {})",
            config.node_id
        );
        tokio::spawn(async move {
            if let Err(e) = run_stun_server(stun_addr).await {
                error!("STUN/TURN server error: {}", e);
            }
        });
    }

    // Connect to peer wpdaemon instances if specified
    for peer_url in config.peers.clone() {
        let url = peer_url.clone();
        tokio::spawn(async move {
            // Small delay to allow peer servers to start if launched simultaneously
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            if let Err(e) = connect_to_peer(url.clone()).await {
                error!("Failed to connect to peer wpdaemon at {}: {}", url, e);
            }
        });
    }

    // Spawn background keep-alive heartbeat task (WPIP-09)
    wpdaemon::connection::start_keepalive_task();

    let address = SocketAddr::new(config.ip, config.port);

    let app = Router::new()
        .route("/sdp", post(wpdaemon::handlers::handle_sdp_offer))
        .route("/peer/sdp", post(wpdaemon::handlers::handle_peer_sdp))
        .layer(middleware::from_fn(rate_limit_middleware))
        .route(
            "/addresses",
            axum::routing::get(wpdaemon::handlers::list_registered_addresses),
        )
        .route(
            "/addresses/:id",
            axum::routing::get(wpdaemon::handlers::resolve_registered_address),
        );

    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(
        "WebRTC audio daemon node {} running on http://{} (max_connections: {})",
        config.node_id, &address, config.max_connections
    );
    if config.turn_enabled {
        info!(
            "STUN/TURN server running on UDP {}",
            SocketAddr::new(config.ip, config.stun_port)
        );
    }
    info!("Waiting for wpclient audio calls & peer daemon mesh connections...");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

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

    /// Unique node ID override for this daemon node.
    #[arg(long)]
    node_id: Option<u64>,

    /// Maximum allowed concurrent WebRTC connections.
    #[arg(long)]
    max_connections: Option<usize>,

    /// Peer wpdaemon URLs to connect to for mesh interconnection.
    #[arg(long)]
    peer: Vec<String>,
}
