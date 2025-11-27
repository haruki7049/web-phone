//! WebTransport audio client entry point.
//!
//! This binary provides a command-line interface for the web-phone
//! audio client. It supports the following commands:
//!
//! - `call` - Start an audio call with the server
//! - `list-devices` - List available audio devices
//!
//! # Usage
//!
//! ```bash
//! # Start a call with default configuration
//! wclient call
//!
//! # Start a call with custom config file
//! wclient --config-path /path/to/config.toml call
//!
//! # List available audio devices
//! wclient list-devices
//! ```

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::info;
use wclient::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};

/// Main entry point for the audio client.
#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().ok();
    tracing_subscriber::fmt::init();
    let args: CLIArgs = CLIArgs::parse();

    CONFIGURATION
        .set(confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running wclient with default Configuration...");
            Configuration::default()
        }))
        .unwrap();

    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or_else(|| anyhow::anyhow!("Failed to get Configuration"))?;

    match args.action {
        Actions::Call => wclient::call::start_call(config).await?,
        Actions::ListDevices => wclient::audio::list_devices()?,
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
}

/// Available client actions.
#[derive(Debug, Clone, clap::Subcommand)]
enum Actions {
    /// Start an audio call with the server.
    ///
    /// Connects to the WebTransport server and begins bidirectional
    /// audio streaming using the default input/output devices.
    Call,
    /// List available audio devices.
    ///
    /// Enumerates all audio input (microphone) and output (speaker)
    /// devices available on the system.
    ListDevices,
}
