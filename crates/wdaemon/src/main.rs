//! WebTransport audio server entry point.
//!
//! This binary provides the server daemon for the web-phone audio
//! transmission system. It accepts WebTransport connections and
//! broadcasts audio between all connected clients.
//!
//! # Usage
//!
//! ```bash
//! # Start server with default configuration
//! wdaemon
//!
//! # Start server with custom config file
//! wdaemon --config-path /path/to/config.toml
//! ```
//!
//! # Configuration
//!
//! The server reads configuration from a TOML file:
//!
//! ```toml
//! ip = "127.0.0.1"
//! port = 15000
//! ```

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::info;
use wdaemon::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
use wtransport::{Endpoint, Identity, ServerConfig};

/// Main entry point for the audio server daemon.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    color_eyre::install().expect("Failed to install color_eyre panic handler");
    tracing_subscriber::fmt::init();

    let args: CLIArgs = CLIArgs::parse();
    CONFIGURATION
        .set(confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running wdaemon with default Configuration...");
            Configuration::default()
        }))
        .unwrap();
    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or("Failed to get Configuration from CONFIGURATION")?;

    let address: SocketAddr = format!("{}:{}", config.ip, config.port).parse()?;

    // Generate self-signed certificate for WebTransport
    let identity = Identity::self_signed(["localhost", "127.0.0.1", &config.ip.to_string()])?;

    let server_config = ServerConfig::builder()
        .with_bind_address(address)
        .with_identity(identity)
        .build();

    let endpoint = Endpoint::server(server_config)?;

    info!("WebTransport audio server running on https://{}", &address);
    info!("Use Ctrl-C to stop this program");
    info!("Waiting for audio clients to connect...");

    // Accept incoming connections
    loop {
        let incoming = endpoint.accept().await;
        tokio::spawn(wdaemon::connection::handle_connection(incoming));
    }
}

/// Command-line arguments for the audio server daemon.
#[derive(Parser)]
struct CLIArgs {
    /// Path to the configuration file.
    #[arg(short, long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,
}
