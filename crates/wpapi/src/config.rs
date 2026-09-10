//! Client configuration module.
//!
//! This module provides configuration types and defaults for the WebRTC
//! audio client.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

/// Default path to the configuration file.
pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "wpclient")
        .expect("Failed to search ProjectDirs for dev.haruki7049.wpclient");
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
    /// Automatically accept incoming call requests without prompting.
    #[serde(default)]
    pub auto_accept: bool,
    /// Use TLS/HTTPS and WSS for secure server communication (default: true).
    #[serde(default = "default_use_tls")]
    pub use_tls: bool,
    /// Name (or substring) of input audio device (microphone) to use.
    #[serde(default)]
    pub input_device: Option<String>,
    /// Name (or substring) of output audio device (speaker) to use.
    #[serde(default)]
    pub output_device: Option<String>,
}

fn default_use_tls() -> bool {
    true
}

fn default_stun_server() -> String {
    "stun:127.0.0.1:3478".to_string()
}

impl Configuration {
    /// Construct full HTTP/HTTPS server base URL according to TLS settings and server IP.
    pub fn server_url(&self) -> String {
        let scheme = if self.use_tls {
            "https"
        } else if self.server_ip.is_loopback() {
            "http"
        } else {
            tracing::warn!(
                "Insecure HTTP transport selected for non-loopback server IP {}",
                self.server_ip
            );
            "http"
        };
        match self.server_ip {
            IpAddr::V4(ip) => format!("{}://{}:{}", scheme, ip, self.server_port),
            IpAddr::V6(ip) => format!("{}://[{}]:{}", scheme, ip, self.server_port),
        }
    }

    /// Construct full WS/WSS WebSocket server URL according to TLS settings and server IP.
    pub fn websocket_url(&self) -> String {
        let scheme = if self.use_tls {
            "wss"
        } else {
            if !self.server_ip.is_loopback() {
                tracing::warn!(
                    "Insecure WS transport selected for non-loopback server IP {}",
                    self.server_ip
                );
            }
            "ws"
        };
        match self.server_ip {
            IpAddr::V4(ip) => format!("{}://{}:{}", scheme, ip, self.server_port),
            IpAddr::V6(ip) => format!("{}://[{}]:{}", scheme, ip, self.server_port),
        }
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            server_ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            server_port: 15000,
            stun_server: default_stun_server(),
            sample_rate: 48000,
            channels: 1,
            allow_echoback: false,
            auto_accept: false,
            use_tls: true,
            input_device: None,
            output_device: None,
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
        assert!(toml_str.contains("sample_rate"));
        assert!(toml_str.contains("channels"));
    }

    #[test]
    fn test_configuration_deserialization() {
        let toml_str = r#"
            server_ip = "192.168.1.1"
            server_port = 16000
            stun_server = "stun:stun.l.google.com:19302"
            sample_rate = 44100
            channels = 2
            allow_echoback = true
            input_device = "Shokz"
            output_device = "MacBook"
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.server_ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(config.server_port, 16000);
        assert_eq!(config.stun_server, "stun:stun.l.google.com:19302");
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channels, 2);
        assert!(config.allow_echoback);
        assert_eq!(config.input_device.as_deref(), Some("Shokz"));
        assert_eq!(config.output_device.as_deref(), Some("MacBook"));
    }

    #[test]
    fn test_configuration_tls_scheme() {
        let mut config = Configuration::default();
        // Default loopback with use_tls = true -> https
        assert_eq!(config.server_url(), "https://127.0.0.1:15000");
        assert_eq!(config.websocket_url(), "wss://127.0.0.1:15000");

        // Non-loopback IP with use_tls = true -> https
        config.server_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(config.server_url(), "https://192.168.1.100:15000");
        assert_eq!(config.websocket_url(), "wss://192.168.1.100:15000");

        // Non-loopback IP with use_tls = false -> http (insecure warning)
        config.use_tls = false;
        assert_eq!(config.server_url(), "http://192.168.1.100:15000");
        assert_eq!(config.websocket_url(), "ws://192.168.1.100:15000");
    }
}
