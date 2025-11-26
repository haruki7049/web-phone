use clap::Parser;
use daemon::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
use std::net::TcpListener;
use std::path::PathBuf;
use tracing::info;

#[tracing::instrument]
fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let address: String = format!("{}:{}", &config.ip, &config.port);
    let server = TcpListener::bind(&address)?;
    info!("WebSocket audio server running on ws://{}", &address);
    info!("Use Ctrl-C to stop this program");
    info!("Waiting for audio clients to connect...");

    loop {
        let (stream, addr) = server.accept()?;
        daemon::read_stream(stream, addr)?;
    }
}

#[derive(Parser)]
struct CLIArgs {
    #[arg(short, long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,
}
