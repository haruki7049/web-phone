use anyhow::Result;
use clap::{Parser, Subcommand};
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::info;
use wpapi::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};

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

    /// Server URL or host address override (e.g. http://127.0.0.1:15000, https://daemon.example.com:15000).
    #[arg(long, short = 's', alias = "server")]
    server_url: Option<String>,

    /// Server IP address or host override.
    #[arg(long)]
    server_ip: Option<String>,

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

    /// Path to file for writing tracing logs without corrupting TUI screen.
    #[arg(long, alias = "log-file")]
    log_file: Option<PathBuf>,

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
        .unwrap_or_else(|| {
            let rand_bytes: [u8; 16] = rand::random();
            hex::encode(rand_bytes)
        });

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

    let log_file_path = args
        .log_file
        .clone()
        .or_else(|| std::env::var_os("WPCLIENT_LOG_FILE").map(PathBuf::from));

    if let Some(path) = log_file_path
        && let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
    {
        tracing_subscriber::fmt()
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false)
            .init();
    }

    let mut loaded_config: Configuration =
        confy::load_path(&args.config_path).unwrap_or_else(|_| {
            info!("Running wpclient with default Configuration...");
            Configuration::default()
        });

    if let Err(e) = loaded_config.normalize() {
        tracing::warn!("Failed to normalize server_url from config file: {}", e);
    }

    if let Some(ref server_url) = args.server_url
        && let Err(e) = loaded_config.parse_and_apply_server_url(server_url)
    {
        tracing::error!("Invalid --server-url argument '{}': {}", server_url, e);
        return Err(anyhow::anyhow!("Invalid --server-url: {}", e));
    }

    if let Some(ref server_ip_raw) = args.server_ip {
        if server_ip_raw.contains("://")
            || (server_ip_raw.contains(':') && server_ip_raw.parse::<IpAddr>().is_err())
        {
            if let Err(e) = loaded_config.parse_and_apply_server_url(server_ip_raw) {
                tracing::error!("Invalid --server-ip argument '{}': {}", server_ip_raw, e);
                return Err(anyhow::anyhow!("Invalid --server-ip: {}", e));
            }
        } else if let Ok(ip) = server_ip_raw.parse::<IpAddr>() {
            loaded_config.server_ip = ip;
            loaded_config.server_host = Some(ip.to_string());
        } else {
            loaded_config.server_host = Some(server_ip_raw.clone());
        }
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
            wpclient::commands::execute_call(&loaded_config, to, keypair).await?;
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
            wpclient::commands::execute_room(&loaded_config, id, keypair).await?;
        }
        Some(Commands::ListAddresses) => {
            wpclient::commands::list_registered_addresses(&loaded_config).await?;
        }
        Some(Commands::ListDevices) => {
            wpclient::commands::list_audio_devices()?;
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
