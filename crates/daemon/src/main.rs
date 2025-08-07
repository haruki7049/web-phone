use clap::Parser;
use tracing::info;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::thread::spawn;
use tungstenite::{Message, accept};

static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");
    let mut config_path: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    config_path.push(filename);
    Mutex::new(config_path)
});

#[tracing::instrument]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args: CLIArgs = CLIArgs::parse();
    let config: Configuration = confy::load_path(&args.config_path).unwrap_or_else(|_| {
        info!("Running web-phone-daemon with default Configuration...");
        Configuration::default()
    });

    let address: String = format!("{}:{}", config.ip, config.port);
    let server = TcpListener::bind(&address)?;
    info!("Running on ws://{}", &address);
    info!("Use Ctrl-C to stop this program");

    loop {
        let (stream, addr) = server.accept()?;
        read_stream(stream, addr)?;
    }
}

#[tracing::instrument]
fn read_stream(stream: TcpStream, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    spawn(move || {
        let mut websocket = accept(stream).unwrap();

        loop {
            let message = websocket.read();

            if message.is_err() {
                break;
            }

            match message.unwrap() {
                Message::Text(utf8_bytes) => {
                    let text: &str = utf8_bytes.as_str();
                    info!(
                        "Message from {}: {}",
                        addr,
                        text.strip_suffix("\n").unwrap()
                    );

                    save_message(text, addr).unwrap();
                }
                _ => (),
            }
        }
    });

    Ok(())
}

#[tracing::instrument]
fn save_message(text: &str, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[derive(Parser)]
struct CLIArgs {
    #[arg(short, long, default_value = DEFAULT_CONFIG_PATH.lock().unwrap().display().to_string())]
    config_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Configuration {
    ip: Ipv4Addr,
    port: u16,
}

impl std::default::Default for Configuration {
    fn default() -> Self {
        Self {
            ip: Ipv4Addr::new(127, 0, 0, 1),
            port: 15000,
        }
    }
}
