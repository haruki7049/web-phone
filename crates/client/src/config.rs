use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

/// Default path to the configuration file
pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-client")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-client");
    let mut result: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    result.push(filename);
    Mutex::new(result)
});

/// Global configuration instance
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configuration {
    pub server_ip: Ipv4Addr,
    pub server_port: u16,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            server_ip: Ipv4Addr::new(127, 0, 0, 1),
            server_port: 15000,
            sample_rate: 48000,
            channels: 1,
        }
    }
}
