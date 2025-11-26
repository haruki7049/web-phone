use clap::Parser;
use client::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
use std::path::PathBuf;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let args: CLIArgs = CLIArgs::parse();

    CONFIGURATION
        .set(confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running web-phone-client with default Configuration...");
            Configuration::default()
        }))
        .unwrap();

    let config: &Configuration = CONFIGURATION.get().ok_or("Failed to get Configuration")?;

    match args.action {
        Actions::Call => client::call::start_call(config).await?,
        Actions::ListDevices => client::audio::list_devices()?,
    }

    Ok(())
}

#[derive(Debug, Parser)]
#[clap(version, author, about = env!("CARGO_PKG_DESCRIPTION"))]
struct CLIArgs {
    #[clap(subcommand)]
    action: Actions,

    #[arg(long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,
}

#[derive(Debug, Clone, clap::Subcommand)]
enum Actions {
    /// Start an audio call with the server
    Call,
    /// List available audio devices
    ListDevices,
}
