//! WebRTC audio daemon and peer mesh entry point.

use axum::{Router, middleware, routing::post};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{error, info};
use wpdaemon::{
    CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH, peer::connect_to_peer, rate_limit_middleware,
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
        loaded_config.server.port = port;
    }
    if let Some(node_id) = args.node_id {
        loaded_config.mesh.node_id = node_id;
    } else if loaded_config.mesh.node_id == 0 {
        loaded_config.mesh.node_id = wpdaemon::config::generate_node_id();
    }
    if let Some(max_conn) = args.max_connections {
        loaded_config.server.max_connections = max_conn;
    }
    if !args.peer.is_empty() {
        loaded_config.mesh.peers.extend(args.peer);
    }

    CONFIGURATION.set(loaded_config).unwrap();

    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or("Failed to get Configuration from CONFIGURATION")?;

    // Connect to peer wpdaemon instances if specified
    for peer_url in config.mesh.peers.clone() {
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

    let address = SocketAddr::new(config.server.ip, config.server.port);

    let app = Router::new()
        .route("/sdp", post(wpdaemon::handlers::handle_sdp_offer))
        .route("/peer/sdp", post(wpdaemon::handlers::handle_peer_sdp))
        .layer(middleware::from_fn(rate_limit_middleware))
        .route(
            "/addresses",
            axum::routing::get(wpdaemon::handlers::list_registered_addresses),
        );

    let listener = tokio::net::TcpListener::bind(address).await?;
    let scheme = if config.tls.tls_cert.is_some() {
        "https"
    } else {
        "http"
    };

    if config.tls.tls_cert.is_none() && !config.server.ip.is_loopback() {
        tracing::warn!(
            "SECURITY WARNING: WebRTC audio daemon node {} is binding to public/external IP {} over unencrypted HTTP! HTTPS/TLS termination is strongly recommended for production deployments.",
            config.mesh.node_id,
            address
        );
    }

    info!(
        "WebRTC audio daemon node {} running on {}://{} (max_connections: {}, allow_anonymous: {})",
        config.mesh.node_id,
        scheme,
        &address,
        config.server.max_connections,
        config.server.allow_anonymous
    );
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
