//! Server configuration module.
//!
//! This module provides configuration types and defaults for the
//! WebRTC audio server daemon, STUN/TURN server, and daemon peer mesh.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
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

/// Server configuration settings (`[server]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Socket address (IP:port) to bind the server service to.
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,
    /// Maximum allowed concurrent WebRTC peer connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// Maximum allowed members per group room.
    #[serde(default = "default_max_room_members")]
    pub max_room_members: usize,
    /// Whether unauthenticated SDP connections are allowed (defaults to false for security).
    #[serde(default)]
    pub allow_anonymous: bool,
}

fn default_bind_address() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 15000))
}

fn default_max_connections() -> usize {
    1000
}

fn default_max_room_members() -> usize {
    50
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            max_connections: default_max_connections(),
            max_room_members: default_max_room_members(),
            allow_anonymous: false,
        }
    }
}

/// TLS configuration settings (`[tls]`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TlsConfig {
    /// Path to TLS certificate file for HTTPS signaling.
    #[serde(default, rename = "cert")]
    pub tls_cert: Option<PathBuf>,
    /// Path to TLS private key file for HTTPS signaling.
    #[serde(default, rename = "key")]
    pub tls_key: Option<PathBuf>,
}

/// Network configuration settings (`[network]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// List of STUN/TURN server URLs for WebRTC ICE candidate gathering.
    #[serde(default = "default_ice_servers")]
    pub ice_servers: Vec<String>,
}

fn default_ice_servers() -> Vec<String> {
    vec!["stun:stun.l.google.com:19302".to_string()]
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            ice_servers: default_ice_servers(),
        }
    }
}

/// Mesh configuration settings (`[mesh]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshConfig {
    /// List of peer wpdaemon signaling addresses to connect to for mesh federation.
    #[serde(default)]
    pub peers: Vec<String>,
    /// Unique identifier for this daemon node.
    #[serde(default = "generate_node_id")]
    pub node_id: u64,
    /// Maximum allowed inter-daemon mesh peer connections.
    #[serde(default = "default_max_mesh_peers")]
    pub max_mesh_peers: usize,
    /// Shared secret or auth token for inter-daemon mesh authentication.
    #[serde(default)]
    pub peer_secret: Option<String>,
}

fn default_max_mesh_peers() -> usize {
    16
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            peers: Vec::new(),
            node_id: generate_node_id(),
            max_mesh_peers: default_max_mesh_peers(),
            peer_secret: None,
        }
    }
}

/// Server configuration for the WebRTC audio daemon.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Configuration {
    /// Server configuration section (`[server]`).
    #[serde(default)]
    pub server: ServerConfig,
    /// TLS configuration section (`[tls]`).
    #[serde(default)]
    pub tls: TlsConfig,
    /// Network configuration section (`[network]`).
    #[serde(default)]
    pub network: NetworkConfig,
    /// Mesh configuration section (`[mesh]`).
    #[serde(default)]
    pub mesh: MeshConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_default() {
        let config = Configuration::default();
        assert_eq!(
            config.server.bind_address,
            "127.0.0.1:15000".parse::<SocketAddr>().unwrap()
        );
        assert!(config.mesh.peers.is_empty());
        assert_ne!(config.mesh.node_id, 0);
        assert_eq!(config.server.max_connections, 1000);
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
        assert!(toml_str.contains("[server]"));
        assert!(toml_str.contains("[mesh]"));
        assert!(toml_str.contains("bind_address"));
    }

    #[test]
    fn test_configuration_deserialization() {
        let toml_str = r#"
            [server]
            bind_address = "192.168.1.1:16000"

            [mesh]
            peers = ["http://192.168.1.2:15000"]
            node_id = 42
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(
            config.server.bind_address,
            "192.168.1.1:16000".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(config.mesh.peers, vec!["http://192.168.1.2:15000"]);
        assert_eq!(config.mesh.node_id, 42);
    }
}
