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

    /// Validate configuration settings.
    pub fn validate(&self) -> Result<(), String> {
        if self.server_port == 0 {
            return Err("Server port cannot be 0".to_string());
        }
        if self.sample_rate < 8000 || self.sample_rate > 96000 {
            return Err(format!(
                "Sample rate {} Hz out of valid range (8000..=96000 Hz)",
                self.sample_rate
            ));
        }
        if self.channels != 1 && self.channels != 2 {
            return Err(format!(
                "Channels {} invalid (must be 1 for mono or 2 for stereo)",
                self.channels
            ));
        }
        if self.stun_server.is_empty() {
            return Err("STUN server URL cannot be empty".to_string());
        }
        if !self.stun_server.starts_with("stun:")
            && !self.stun_server.starts_with("stuns:")
            && !self.stun_server.starts_with("turn:")
            && !self.stun_server.starts_with("turns:")
        {
            return Err(format!(
                "Invalid STUN/TURN URL format: '{}' (must start with stun:, stuns:, turn:, or turns:)",
                self.stun_server
            ));
        }
        Ok(())
    }
}

/// Fluent builder for constructing `Configuration` instances with validation.
#[derive(Debug, Clone, Default)]
pub struct ConfigurationBuilder {
    config: Configuration,
}

impl ConfigurationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn server_ip(mut self, ip: IpAddr) -> Self {
        self.config.server_ip = ip;
        self
    }

    pub fn server_port(mut self, port: u16) -> Self {
        self.config.server_port = port;
        self
    }

    pub fn stun_server(mut self, stun: impl Into<String>) -> Self {
        self.config.stun_server = stun.into();
        self
    }

    pub fn sample_rate(mut self, rate: u32) -> Self {
        self.config.sample_rate = rate;
        self
    }

    pub fn channels(mut self, channels: u16) -> Self {
        self.config.channels = channels;
        self
    }

    pub fn use_tls(mut self, use_tls: bool) -> Self {
        self.config.use_tls = use_tls;
        self
    }

    pub fn build(self) -> Result<Configuration, String> {
        self.config.validate()?;
        Ok(self.config)
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

    #[test]
    fn test_configuration_validation() {
        let mut config = Configuration::default();
        assert!(config.validate().is_ok());

        config.server_port = 0;
        assert!(config.validate().is_err());

        config.server_port = 15000;
        config.sample_rate = 500;
        assert!(config.validate().is_err());

        config.sample_rate = 48000;
        config.channels = 5;
        assert!(config.validate().is_err());

        config.channels = 1;
        config.stun_server = "invalid_scheme".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_configuration_builder() {
        let config = ConfigurationBuilder::new()
            .server_port(18000)
            .sample_rate(44100)
            .channels(2)
            .stun_server("stun:stun.l.google.com:19302")
            .build()
            .expect("Build should succeed");

        assert_eq!(config.server_port, 18000);
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channels, 2);
        assert_eq!(config.stun_server, "stun:stun.l.google.com:19302");
    }
}
