use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::thread::spawn;
use tracing::{debug, info};
use tungstenite::{Message, accept};

pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");
    let mut config_path: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    config_path.push(filename);
    Mutex::new(config_path)
});
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

#[tracing::instrument]
pub fn read_stream(stream: TcpStream, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
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
                Message::Close(v) => match v {
                    Some(close_frame) => {
                        info!("{} is closed by: {}", addr, close_frame);
                        break;
                    }
                    None => {
                        info!("{} is closed without any reason", addr);
                        break;
                    }
                },
                _ => (),
            }
        }
    });

    Ok(())
}

#[tracing::instrument]
fn save_message(text: &str, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let config: &Configuration = CONFIGURATION
        .get()
        .ok_or("Failed to get Configuration from CONFIGURATION")?;

    debug!("config: {:?}", config);

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub log_dir: PathBuf,
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl std::default::Default for Configuration {
    fn default() -> Self {
        Self {
            log_dir: default_log_dir(),
            ip: Ipv4Addr::new(127, 0, 0, 1),
            port: 15000,
        }
    }
}

fn default_log_dir() -> PathBuf {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");

    proj_dirs.data_dir().to_path_buf()
}
