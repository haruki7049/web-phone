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

    /// Anonymous mode: use disposable temporary key pair in memory (alias: --ephemeral).
    #[arg(long, alias = "ephemeral")]
    anonymous: bool,

    /// Passphrase for decrypting or creating WPIP-14 encrypted keystore.
    #[arg(long)]
    passphrase: Option<String>,

    /// Nostr secret key (nsec1... Bech32 or 64-char Hex) for Nostr/secp256k1 identity (WPIP-16).
    #[arg(long, short = 'n', alias = "nsec")]
    nostr_key: Option<String>,

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

        /// Anonymous mode: use disposable temporary key pair in memory (alias: --ephemeral)
        #[arg(long, alias = "ephemeral")]
        anonymous: bool,

        /// Passphrase for decrypting or creating WPIP-14 encrypted keystore
        #[arg(long)]
        passphrase: Option<String>,

        /// Nostr secret key (nsec1... Bech32 or 64-char Hex) for Nostr/secp256k1 identity (WPIP-16)
        #[arg(long, short = 'n', alias = "nsec")]
        nostr_key: Option<String>,
    },
    /// Join an SFU group audio room
    Room {
        /// SFU Room ID to join
        #[arg(long)]
        id: String,

        /// Anonymous mode: use disposable temporary key pair in memory (alias: --ephemeral)
        #[arg(long, alias = "ephemeral")]
        anonymous: bool,

        /// Passphrase for decrypting or creating WPIP-14 encrypted keystore
        #[arg(long)]
        passphrase: Option<String>,

        /// Nostr secret key (nsec1... Bech32 or 64-char Hex) for Nostr/secp256k1 identity (WPIP-16)
        #[arg(long, short = 'n', alias = "nsec")]
        nostr_key: Option<String>,
    },
    /// List all registered peer user addresses connected to the daemon
    ListAddresses,
    /// List available audio input and output devices
    ListDevices,
}

fn load_or_create_client_keypair(
    anonymous: bool,
    passphrase_override: Option<&str>,
    nostr_key_override: Option<&str>,
) -> wpapi::UserKeypair {
    let nostr_key_input = nostr_key_override
        .map(|s| s.to_string())
        .or_else(|| std::env::var("WPCLIENT_NOSTR_KEY").ok());

    let keystore_path = wpapi::get_default_keystore_path();
    let passphrase = passphrase_override
        .map(|s| s.to_string())
        .or_else(|| std::env::var("WPCLIENT_PASSPHRASE").ok())
        .unwrap_or_else(|| "default_wpclient_passphrase_key_12345".to_string());

    if let Some(ref nostr_key) = nostr_key_input {
        match wpapi::UserKeypair::from_nostr_key(nostr_key) {
            Ok(keypair) => {
                info!(
                    "Loaded Nostr secp256k1 identity key: {}",
                    keypair.public_key_address()
                );
                if let Err(e) =
                    wpapi::save_encrypted_keystore(&keypair, &passphrase, &keystore_path)
                {
                    tracing::warn!("Failed to save Nostr encrypted keystore: {}", e);
                }
                return keypair;
            }
            Err(e) => {
                tracing::error!("Invalid Nostr key provided ({}), falling back...", e);
            }
        }
    }

    if anonymous {
        info!("Running in anonymous/ephemeral mode with temporary Ed25519 identity key...");
        return wpapi::UserKeypair::generate();
    }

    if keystore_path.exists() {
        match wpapi::load_encrypted_keystore(&passphrase, &keystore_path) {
            Ok(keypair) => {
                info!(
                    "Loaded persistent UserAddress from encrypted keystore: {}",
                    keypair.public_key_address()
                );
                keypair
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to decrypt keystore ({}), falling back to temporary keypair...",
                    err
                );
                wpapi::UserKeypair::generate()
            }
        }
    } else {
        let keypair = wpapi::UserKeypair::generate();
        if let Err(e) = wpapi::save_encrypted_keystore(&keypair, &passphrase, &keystore_path) {
            tracing::warn!("Failed to save new encrypted keystore: {}", e);
        } else {
            info!(
                "Created new WPIP-14 encrypted keystore at {}",
                keystore_path.display()
            );
        }
        keypair
    }
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

    let global_anonymous = args.anonymous;
    let global_passphrase = args.passphrase.as_deref();
    let global_nostr_key = args.nostr_key.as_deref();

    match args.command {
        Some(Commands::Call {
            to,
            auto_accept,
            anonymous,
            passphrase,
            nostr_key,
        }) => {
            if auto_accept {
                loaded_config.auto_accept = true;
            }
            let is_anon = global_anonymous || anonymous;
            let pass = passphrase.as_deref().or(global_passphrase);
            let nk = nostr_key.as_deref().or(global_nostr_key);
            let keypair = load_or_create_client_keypair(is_anon, pass, nk);

            if let Some(target) = to {
                info!("Starting direct call to target: {}", target);
                wpapi::call::start_call(&loaded_config, Some(UserAddress::new(target))).await?;
            } else {
                wpclient::tui::run_tui_with_keypair(loaded_config, Some(keypair)).await?;
            }
        }
        Some(Commands::Room {
            id,
            anonymous,
            passphrase,
            nostr_key,
        }) => {
            let is_anon = global_anonymous || anonymous;
            let pass = passphrase.as_deref().or(global_passphrase);
            let nk = nostr_key.as_deref().or(global_nostr_key);
            let keypair = load_or_create_client_keypair(is_anon, pass, nk);
            info!(
                "Joining room: {} with address {}",
                id,
                keypair.public_key_address()
            );
            wpapi::call::start_room_call(&loaded_config, UserAddress::new(id)).await?;
        }
        Some(Commands::ListAddresses) => {
            list_registered_addresses(&loaded_config).await?;
        }
        Some(Commands::ListDevices) => {
            list_audio_devices()?;
        }
        None => {
            let keypair = load_or_create_client_keypair(
                global_anonymous,
                global_passphrase,
                global_nostr_key,
            );
            wpclient::tui::run_tui_with_keypair(loaded_config, Some(keypair)).await?;
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
            if let Ok(desc) = dev.description() {
                println!("  • {}", desc.name());
            }
        }
    }
    println!("\nAudio Output Devices (Speakers):");
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(desc) = dev.description() {
                println!("  • {}", desc.name());
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
