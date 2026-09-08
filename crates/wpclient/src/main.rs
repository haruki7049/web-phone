//! WebRTC audio client GUI application entry point.
//!
//! This binary provides the graphical desktop user interface (Iced) for the web-phone
//! audio client.
//!
//! # Usage
//!
//! ```bash
//! # Launch desktop GUI
//! wpclient
//!
//! # Show version
//! wpclient --version
//!
//! # Show help
//! wpclient --help
//! ```

use anyhow::Result;
use clap::Parser;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::info;
use wpapi::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};

/// Command-line arguments for the audio client GUI.
#[derive(Debug, Parser)]
#[clap(
    version,
    author,
    about = "Decentralized WebRTC audio client featuring full interactive GUI (Iced)."
)]
struct CLIArgs {
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

/// Main entry point for the audio client GUI.
#[tokio::main]
async fn main() -> Result<()> {
    let handle = tokio::runtime::Handle::current();
    let _ = wpclient::gui::TOKIO_HANDLE.set(handle);

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

    CONFIGURATION.set(loaded_config).unwrap();

    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or_else(|| anyhow::anyhow!("Failed to get Configuration"))?;

    // Launch Iced GUI directly
    wpclient::gui::run_gui(config.clone())?;

    Ok(())
}
