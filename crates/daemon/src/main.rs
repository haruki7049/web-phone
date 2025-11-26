use clap::Parser;
use daemon::{Configuration, CONFIGURATION, DEFAULT_CONFIG_PATH};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::info;
use wtransport::{Endpoint, Identity, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let address: SocketAddr = format!("{}:{}", &config.ip, &config.port).parse()?;

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
        tokio::spawn(daemon::connection::handle_connection(incoming));
    }
}

#[derive(Parser)]
struct CLIArgs {
    #[arg(short, long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,
}
