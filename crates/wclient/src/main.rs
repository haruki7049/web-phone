//! WebRTC audio client entry point.
//!
//! This binary provides a command-line interface for the web-phone
//! audio client. It supports the following commands:
//!
//! - `call` - Start an audio call with the server via WebRTC
//! - `list-devices` - List available audio devices
//!
//! # Usage
//!
//! ```bash
//! # Start a call with default configuration
//! wclient call
//!
//! # Start a call with custom server IP and port
//! wclient --server-ip 127.0.0.1 --server-port 15000 call
//!
//! # Start a call with a custom user IPv6 address for recognition
//! wclient --user-address 2001:db8::1 call
//!
//! # List available audio devices
//! wclient list-devices
//! ```

use anyhow::Result;
use clap::Parser;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::info;
use wclient::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH, UserAddress};

/// Main entry point for the audio client.
#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().expect("Failed to install color_eyre panic handler");
    tracing_subscriber::fmt::init();
    let args: CLIArgs = CLIArgs::parse();

    let mut loaded_config: Configuration =
        confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running wclient with default Configuration...");
            Configuration::default()
        });

    if let Some(server_ip) = args.server_ip {
        loaded_config.server_ip = server_ip;
    }
    if let Some(server_port) = args.server_port {
        loaded_config.server_port = server_port;
    }
    if let Some(user_address) = args.user_address {
        loaded_config.user_address = user_address;
    }
    if let Some(stun_server) = args.stun_server {
        loaded_config.stun_server = stun_server;
    }

    CONFIGURATION.set(loaded_config).unwrap();

    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or_else(|| anyhow::anyhow!("Failed to get Configuration"))?;

    match args.action {
        Actions::Call { to } => wclient::call::start_call(config, to).await?,
        Actions::ListAddresses => list_registered_addresses(config).await?,
        Actions::ListDevices => wclient::audio::list_devices()?,
    }

    Ok(())
}

/// Query and display registered wclient addresses from wdaemon.
async fn list_registered_addresses(config: &Configuration) -> Result<()> {
    let server_url = match config.server_ip {
        std::net::IpAddr::V4(ip) => format!("http://{}:{}", ip, config.server_port),
        std::net::IpAddr::V6(ip) => format!("http://[{}]:{}", ip, config.server_port),
    };
    let endpoint = format!("{}/addresses", server_url);
    let client = reqwest::Client::new();
    let resp = client.get(&endpoint).send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("Failed to fetch addresses from server: {}", resp.status());
    }

    let addresses: Vec<UserAddress> = resp.json().await?;
    if addresses.is_empty() {
        println!("No registered wclient temporary user IDs currently connected.");
    } else {
        println!(
            "Registered wclient temporary SHA-256 user IDs ({} total):",
            addresses.len()
        );
        for (idx, addr) in addresses.iter().enumerate() {
            println!("  {}. {} (Short: {})", idx + 1, addr, addr.short_id());
        }
    }

    Ok(())
}

/// Command-line arguments for the audio client.
#[derive(Debug, Parser)]
#[clap(version, author, about = env!("CARGO_PKG_DESCRIPTION"))]
struct CLIArgs {
    /// The action to perform.
    #[clap(subcommand)]
    action: Actions,

    /// Path to the configuration file.
    #[arg(long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,

    /// Server IP address override.
    #[arg(long)]
    server_ip: Option<IpAddr>,

    /// Server port override.
    #[arg(long)]
    server_port: Option<u16>,

    /// User address override for wclient recognition.
    #[arg(long)]
    user_address: Option<UserAddress>,

    /// STUN server URL override.
    #[arg(long)]
    stun_server: Option<String>,
}

/// Available client actions.
#[derive(Debug, Clone, clap::Subcommand)]
enum Actions {
    /// Start a 1-to-1 audio call or standby to receive calls via WebRTC.
    Call {
        /// Target wclient temporary SHA-256 user ID (or prefix) to call directly (optional).
        #[arg(long, short = 't')]
        to: Option<UserAddress>,
    },
    /// List all registered wclient temporary user IDs connected to wdaemon.
    ListAddresses,
    /// List available audio devices.
    ListDevices,
}
