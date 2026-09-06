//! Client configuration module.
//!
//! This module provides configuration types and defaults for the WebRTC
//! audio client.

use crate::address::UserAddress;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

/// Default path to the configuration file.
pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-client")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-client");
    let mut result: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    result.push(filename);
    Mutex::new(result)
});

/// Global configuration instance.
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Client configuration for the WebRTC audio client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configuration {
    /// IP address of the audio server to connect to (supports IPv4 or IPv6).
    pub server_ip: IpAddr,
    /// Port number of the audio server.
    pub server_port: u16,
    /// Address information to recognize this wclient user (IPv6).
    #[serde(default)]
    pub user_address: UserAddress,
    /// STUN server URL for NAT traversal.
    #[serde(default = "default_stun_server")]
    pub stun_server: String,
    /// Audio sample rate in Hz (e.g., 48000, 44100).
    pub sample_rate: u32,
    /// Number of audio channels (1 for mono, 2 for stereo).
    pub channels: u16,
    /// Allow echo back (hear your own voice).
    #[serde(default)]
    pub allow_echoback: bool,
}

fn default_stun_server() -> String {
    "stun:127.0.0.1:3478".to_string()
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            server_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            server_port: 15000,
            user_address: UserAddress::default(),
            stun_server: default_stun_server(),
            sample_rate: 48000,
            channels: 1,
            allow_echoback: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_default() {
        let config = Configuration::default();
        assert_eq!(config.server_ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.server_port, 15000);
        assert_eq!(config.user_address.id.len(), 64);
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.channels, 1);
        assert!(!config.allow_echoback);
        assert_eq!(config.stun_server, "stun:127.0.0.1:3478");
    }

    #[test]
    fn test_configuration_serialization() {
        let config = Configuration::default();
        let toml_str = toml::to_string(&config).expect("Failed to serialize configuration");
        assert!(toml_str.contains("server_ip"));
        assert!(toml_str.contains("server_port"));
        assert!(toml_str.contains("user_address"));
        assert!(toml_str.contains("sample_rate"));
        assert!(toml_str.contains("channels"));
    }

    #[test]
    fn test_configuration_deserialization() {
        let toml_str = r#"
            server_ip = "192.168.1.1"
            server_port = 16000
            user_address = "a1b2c3d4e5f607080900"
            stun_server = "stun:stun.l.google.com:19302"
            sample_rate = 44100
            channels = 2
            allow_echoback = true
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.server_ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(config.server_port, 16000);
        assert_eq!(
            config.user_address,
            UserAddress::new("a1b2c3d4e5f607080900")
        );
        assert_eq!(config.stun_server, "stun:stun.l.google.com:19302");
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channels, 2);
        assert!(config.allow_echoback);
    }
}
