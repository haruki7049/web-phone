//! Server configuration module.
//!
//! This module provides configuration types and defaults for the
//! WebRTC audio server daemon, STUN/TURN server, and daemon peer mesh.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

/// Default path to the configuration file.
pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");
    let mut config_path: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    config_path.push(filename);
    Mutex::new(config_path)
});

/// Global configuration instance.
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Server configuration for the WebRTC audio daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configuration {
    /// IP address to bind the server and STUN/TURN services to.
    pub ip: Ipv4Addr,
    /// Port number for HTTP/WebRTC signaling and peer API.
    pub port: u16,
    /// UDP port for STUN/TURN service.
    pub stun_port: u16,
    /// Whether STUN/TURN service is enabled.
    #[serde(default = "default_true")]
    pub turn_enabled: bool,
    /// List of peer wdaemon signaling addresses to connect to for mesh federation.
    #[serde(default)]
    pub peers: Vec<String>,
    /// Unique identifier for this daemon node.
    #[serde(default)]
    pub node_id: u64,
}

fn default_true() -> bool {
    true
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            ip: Ipv4Addr::new(127, 0, 0, 1),
            port: 15000,
            stun_port: 3478,
            turn_enabled: true,
            peers: Vec::new(),
            node_id: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_default() {
        let config = Configuration::default();
        assert_eq!(config.ip, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(config.port, 15000);
        assert_eq!(config.stun_port, 3478);
        assert!(config.turn_enabled);
        assert!(config.peers.is_empty());
    }

    #[test]
    fn test_configuration_serialization() {
        let config = Configuration::default();
        let toml_str = toml::to_string(&config).expect("Failed to serialize configuration");
        assert!(toml_str.contains("ip"));
        assert!(toml_str.contains("port"));
        assert!(toml_str.contains("stun_port"));
    }

    #[test]
    fn test_configuration_deserialization() {
        let toml_str = r#"
            ip = "192.168.1.1"
            port = 16000
            stun_port = 3479
            turn_enabled = true
            peers = ["http://192.168.1.2:15000"]
            node_id = 42
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.ip, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(config.port, 16000);
        assert_eq!(config.stun_port, 3479);
        assert_eq!(config.peers, vec!["http://192.168.1.2:15000"]);
        assert_eq!(config.node_id, 42);
    }
}
