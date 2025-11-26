use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use tokio::sync::broadcast;
use wtransport::SendStream;

pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");
    let mut config_path: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    config_path.push(filename);
    Mutex::new(config_path)
});
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Type alias for a client's send stream
pub type ClientSender = std::sync::Arc<tokio::sync::Mutex<SendStream>>;

/// Broadcast channel for audio data
pub static AUDIO_BROADCAST: LazyLock<broadcast::Sender<Vec<u8>>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(100);
    tx
});

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl std::default::Default for Configuration {
    fn default() -> Self {
        Self {
            ip: Ipv4Addr::new(127, 0, 0, 1),
            port: 15000,
        }
    }
}
