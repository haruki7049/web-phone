//! Server configuration module.
//!
//! This module provides configuration types and defaults for the
//! WebRTC audio server daemon, STUN/TURN server, and daemon peer mesh.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

/// Default path to the configuration file.
pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "wpdaemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.wpdaemon");
    let mut config_path: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    config_path.push(filename);
    Mutex::new(config_path)
});

/// Global configuration instance.
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

use rand::RngCore;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NODE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique 64-bit node ID based on time, process ID, and atomic counter.
pub fn generate_node_id() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let count = NODE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();

    let mut hasher = Sha256::new();
    hasher.update(format!("{}-{}-{}", nanos, pid, count).as_bytes());
    let hash = hasher.finalize();

    let bytes: [u8; 8] = hash[..8].try_into().unwrap();
    u64::from_le_bytes(bytes)
}

/// Server configuration for the WebRTC audio daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configuration {
    /// IP address to bind the server and STUN/TURN services to (supports IPv4 or IPv6).
    pub ip: IpAddr,
    /// Port number for HTTP/WebRTC signaling and peer API.
    pub port: u16,
    /// UDP port for STUN/TURN service.
    pub stun_port: u16,
    /// Whether STUN/TURN service is enabled.
    #[serde(default = "default_true")]
    pub turn_enabled: bool,
    /// Secret key for STUN/TURN Ephemeral HMAC-SHA1 Token Authentication (WPIP-10).
    #[serde(default = "default_turn_server_secret")]
    pub turn_server_secret: String,
    /// List of peer wpdaemon signaling addresses to connect to for mesh federation.
    #[serde(default)]
    pub peers: Vec<String>,
    /// Unique identifier for this daemon node.
    #[serde(default = "generate_node_id")]
    pub node_id: u64,
    /// Maximum allowed concurrent WebRTC peer connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// Maximum allowed inter-daemon mesh peer connections.
    #[serde(default = "default_max_mesh_peers")]
    pub max_mesh_peers: usize,
    /// Maximum allowed members per group room.
    #[serde(default = "default_max_room_members")]
    pub max_room_members: usize,
}

fn default_true() -> bool {
    true
}

/// Generate a cryptographically secure 256-bit random hex secret for TURN authentication.
pub fn generate_random_turn_secret() -> String {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    hex::encode(key)
}

static DEFAULT_DYNAMIC_TURN_SECRET: LazyLock<String> = LazyLock::new(generate_random_turn_secret);

/// Get the active TURN server secret key (from CONFIGURATION or fallback lazy static secret).
pub fn get_turn_server_secret() -> Vec<u8> {
    CONFIGURATION
        .get()
        .map(|c| c.turn_server_secret.as_bytes().to_vec())
        .unwrap_or_else(|| DEFAULT_DYNAMIC_TURN_SECRET.as_bytes().to_vec())
}

fn default_turn_server_secret() -> String {
    generate_random_turn_secret()
}

fn default_max_connections() -> usize {
    1000
}

fn default_max_mesh_peers() -> usize {
    16
}

fn default_max_room_members() -> usize {
    50
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 15000,
            stun_port: 3478,
            turn_enabled: true,
            turn_server_secret: default_turn_server_secret(),
            peers: Vec::new(),
            node_id: generate_node_id(),
            max_connections: default_max_connections(),
            max_mesh_peers: default_max_mesh_peers(),
            max_room_members: default_max_room_members(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_default() {
        let config = Configuration::default();
        assert_eq!(config.ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.port, 15000);
        assert_eq!(config.stun_port, 3478);
        assert!(config.turn_enabled);
        assert!(config.peers.is_empty());
        assert_ne!(config.node_id, 0);
        assert_eq!(config.max_connections, 1000);
    }

    #[test]
    fn test_generate_node_id_uniqueness() {
        let id1 = generate_node_id();
        let id2 = generate_node_id();
        assert_ne!(id1, 0);
        assert_ne!(id2, 0);
        assert_ne!(id1, id2);
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
        assert_eq!(config.ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(config.port, 16000);
        assert_eq!(config.stun_port, 3479);
        assert_eq!(config.peers, vec!["http://192.168.1.2:15000"]);
        assert_eq!(config.node_id, 42);
    }
}
