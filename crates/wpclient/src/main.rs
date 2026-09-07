//! WebRTC audio client entry point.
//!
//! This binary provides a command-line interface for the web-phone
//! audio client. It supports the following commands:
//!
//! - `call` - Start a 1-to-1 audio call or standby to receive incoming calls via WebRTC
//! - `room` - Join a group audio room (WPIP-08)
//! - `list-addresses` - List all registered user addresses connected to `wpdaemon`
//! - `list-devices` - List available audio input/output devices
//!
//! # Usage
//!
//! ```bash
//! # Start a direct 1-to-1 call
//! wpclient call --to <TARGET_USER_ID>
//!
//! # Join a group audio room
//! wpclient room --id <ROOM_ID>
//!
//! # List connected user addresses
//! wpclient list-addresses
//!
//! # List available audio devices
//! wpclient list-devices
//! ```

use anyhow::Result;
use clap::Parser;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::info;
use wpapi::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH, UserAddress};

/// Main entry point for the audio client.
#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().expect("Failed to install color_eyre panic handler");
    tracing_subscriber::fmt::init();
    let args: CLIArgs = CLIArgs::parse();

    let mut loaded_config: Configuration =
        confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running wpclient with default Configuration...");
            Configuration::default()
        });

    if let Some(server_ip) = args.server_ip {
        loaded_config.server_ip = server_ip;
    }
    if let Some(server_port) = args.server_port {
        loaded_config.server_port = server_port;
    }
    if let Some(stun_server) = args.stun_server {
        loaded_config.stun_server = stun_server;
    }
    if let Some(input_device) = args.input_device {
        loaded_config.input_device = Some(input_device);
    }
    if let Some(output_device) = args.output_device {
        loaded_config.output_device = Some(output_device);
    }

    if let Actions::Call {
        auto_accept: true, ..
    } = &args.action
    {
        loaded_config.auto_accept = true;
    }

    CONFIGURATION.set(loaded_config).unwrap();

    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or_else(|| anyhow::anyhow!("Failed to get Configuration"))?;

    match args.action {
        Actions::Call { to, .. } => wpclient::call::start_call(config, to).await?,
        Actions::Room { id } => wpclient::call::start_room_call(config, id).await?,
        Actions::ListAddresses => list_registered_addresses(config).await?,
        Actions::ListDevices => wpclient::audio::list_devices()?,
    }

    Ok(())
}

/// Query and display registered wpclient addresses from wpdaemon.
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
        println!("No registered wpclient temporary user IDs currently connected.");
    } else {
        println!(
            "Registered wpclient temporary SHA-256 user IDs ({} total):",
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

    /// STUN server URL override.
    #[arg(long)]
    stun_server: Option<String>,

    /// Input device (microphone) name or substring override.
    #[arg(long, short = 'i')]
    input_device: Option<String>,

    /// Output device (speaker) name or substring override.
    #[arg(long, short = 'o')]
    output_device: Option<String>,
}

/// Available client actions.
#[derive(Debug, Clone, clap::Subcommand)]
enum Actions {
    /// Start a 1-to-1 audio call or standby to receive calls via WebRTC.
    Call {
        /// Target wpclient temporary SHA-256 user ID (or prefix) to call directly (optional).
        #[arg(long, short = 't')]
        to: Option<UserAddress>,

        /// Automatically accept incoming call requests without prompting.
        #[arg(long, short = 'y', default_value_t = false)]
        auto_accept: bool,
    },
    /// Join a group audio room (WPIP-08).
    Room {
        /// Target room SHA-256 ID (or room key string) to join.
        #[arg(long, short = 'r')]
        id: UserAddress,
    },
    /// List all registered wpclient temporary user IDs connected to wpdaemon.
    ListAddresses,
    /// List available audio devices.
    ListDevices,
}
