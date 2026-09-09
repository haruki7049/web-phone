use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::info;
use wpapi::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH, UserAddress};

/// Command-line arguments for the audio client TUI.
#[derive(Debug, Parser)]
#[clap(
    version,
    author,
    about = "Decentralized WebRTC audio client featuring interactive TUI (Ratatui)."
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

    /// Automatically accept incoming calls without interactive prompt.
    #[arg(long, short = 'y')]
    auto_accept: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start a 1-to-1 call session or standby mode
    Call {
        /// Target SHA-256 User ID or Short ID to call
        #[arg(long, short = 't')]
        to: Option<String>,

        /// Automatically accept incoming call requests without prompting
        #[arg(long, short = 'y')]
        auto_accept: bool,
    },
    /// Join an SFU group audio room
    Room {
        /// SFU Room ID to join
        #[arg(long)]
        id: String,
    },
    /// List all registered peer user addresses connected to the daemon
    ListAddresses,
    /// List available audio input and output devices
    ListDevices,
}

/// Main entry point for the audio client TUI.
#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().expect("Failed to install color_eyre panic handler");
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
    if args.auto_accept {
        loaded_config.auto_accept = true;
    }

    CONFIGURATION.set(loaded_config.clone()).unwrap();

    match args.command {
        Some(Commands::Call { to, auto_accept }) => {
            if auto_accept {
                loaded_config.auto_accept = true;
            }
            if let Some(target) = to {
                info!("Starting direct call to target: {}", target);
                wpapi::call::start_call(&loaded_config, Some(UserAddress::new(target))).await?;
            } else {
                wpclient::tui::run_tui(loaded_config).await?;
            }
        }
        Some(Commands::Room { id }) => {
            info!("Joining room: {}", id);
            wpapi::call::start_room_call(&loaded_config, UserAddress::new(id)).await?;
        }
        Some(Commands::ListAddresses) => {
            list_registered_addresses(&loaded_config).await?;
        }
        Some(Commands::ListDevices) => {
            list_audio_devices()?;
        }
        None => {
            wpclient::tui::run_tui(loaded_config).await?;
        }
    }

    Ok(())
}

fn list_audio_devices() -> Result<()> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    println!("Audio Input Devices (Microphones):");
    if let Ok(devices) = host.input_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                println!("  • {}", name);
            }
        }
    }
    println!("\nAudio Output Devices (Speakers):");
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                println!("  • {}", name);
            }
        }
    }
    Ok(())
}

async fn list_registered_addresses(config: &Configuration) -> Result<()> {
    let endpoint = format!(
        "http://{}:{}/addresses",
        config.server_ip, config.server_port
    );
    println!(
        "Fetching registered addresses from daemon at {}...",
        endpoint
    );
    let client = reqwest::Client::new();
    let resp = client.get(&endpoint).send().await?;
    if resp.status().is_success() {
        let addrs: Vec<UserAddress> = resp.json().await?;
        println!("Registered User Addresses (Total: {}):", addrs.len());
        for addr in addrs {
            println!("  • Short ID: {} | Full: {}", addr.short_id(), addr);
        }
    } else {
        println!("Server returned status code: {}", resp.status());
    }
    Ok(())
}
