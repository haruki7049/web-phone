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

/// Server configuration settings (`[server]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// IP address of the audio server to connect to (supports IPv4 or IPv6).
    #[serde(default = "default_server_ip")]
    pub ip: IpAddr,
    /// Port number of the audio server.
    #[serde(default = "default_server_port")]
    pub port: u16,
    /// Hostname or domain name override for the audio server (e.g. "localhost", "daemon.example.com").
    #[serde(default)]
    pub host: Option<String>,
    /// Server URL override string (e.g. "http://127.0.0.1:15000", "https://daemon.example.com:15000").
    #[serde(default, rename = "url")]
    pub url_override: Option<String>,
    /// Use TLS/HTTPS and WSS for secure server communication (default: true).
    #[serde(default = "default_use_tls")]
    pub use_tls: bool,
}

fn default_server_ip() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
}

fn default_server_port() -> u16 {
    15000
}

fn default_use_tls() -> bool {
    true
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            ip: default_server_ip(),
            port: default_server_port(),
            host: None,
            url_override: None,
            use_tls: default_use_tls(),
        }
    }
}

/// Network configuration settings (`[network]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// STUN server URL for NAT traversal.
    #[serde(default = "default_stun_server")]
    pub stun_server: String,
}

fn default_stun_server() -> String {
    "stun:stun.l.google.com:19302".to_string()
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            stun_server: default_stun_server(),
        }
    }
}

/// Audio configuration settings (`[audio]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Audio sample rate in Hz (e.g., 48000, 44100).
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    /// Number of audio channels (1 for mono, 2 for stereo).
    #[serde(default = "default_channels")]
    pub channels: u16,
    /// Allow echo back (hear your own voice).
    #[serde(default)]
    pub allow_echoback: bool,
    /// Name (or substring) of input audio device (microphone) to use.
    #[serde(default)]
    pub input_device: Option<String>,
    /// Name (or substring) of output audio device (speaker) to use.
    #[serde(default)]
    pub output_device: Option<String>,
}

fn default_sample_rate() -> u32 {
    48000
}

fn default_channels() -> u16 {
    1
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            channels: default_channels(),
            allow_echoback: false,
            input_device: None,
            output_device: None,
        }
    }
}

/// Client configuration settings (`[client]`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientConfig {
    /// Automatically accept incoming call requests without prompting.
    #[serde(default)]
    pub auto_accept: bool,
}

/// Client configuration for the WebRTC audio client.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Configuration {
    /// Server configuration section (`[server]`).
    #[serde(default)]
    pub server: ServerConfig,
    /// Network configuration section (`[network]`).
    #[serde(default)]
    pub network: NetworkConfig,
    /// Audio configuration section (`[audio]`).
    #[serde(default)]
    pub audio: AudioConfig,
    /// Client behavior configuration section (`[client]`).
    #[serde(default)]
    pub client: ClientConfig,
}

impl Configuration {
    /// Parse and apply a server URL or host:port specification string.
    ///
    /// Accepts formats like:
    /// - `http://127.0.0.1:15000`
    /// - `https://daemon.example.com:8443`
    /// - `http://localhost:15000`
    /// - `ws://127.0.0.1:15000`
    /// - `wss://daemon.example.com:8443`
    /// - `127.0.0.1:15000`
    /// - `127.0.0.1`
    /// - `daemon.example.com:15000`
    pub fn parse_and_apply_server_url(&mut self, input: &str) -> Result<(), String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Server URL cannot be empty".to_string());
        }

        let (url_to_parse, scheme_specified) = if trimmed.contains("://") {
            (trimmed.to_string(), true)
        } else {
            (format!("http://{}", trimmed), false)
        };

        let parsed = reqwest::Url::parse(&url_to_parse)
            .map_err(|e| format!("Invalid server URL format '{}': {}", input, e))?;

        if scheme_specified {
            match parsed.scheme() {
                "https" | "wss" => {
                    self.server.use_tls = true;
                }
                "http" | "ws" => {
                    self.server.use_tls = false;
                }
                other => {
                    return Err(format!(
                        "Unsupported URL scheme '{}' (must be http, https, ws, or wss)",
                        other
                    ));
                }
            }
        }

        if let Some(host_str) = parsed.host_str() {
            let clean_host = host_str.trim_matches('[').trim_matches(']').to_string();
            if let Ok(ip) = clean_host.parse::<IpAddr>() {
                self.server.ip = ip;
                self.server.host = Some(clean_host);
            } else {
                if clean_host.eq_ignore_ascii_case("localhost") {
                    self.server.ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
                }
                self.server.host = Some(clean_host);
            }
        } else {
            return Err(format!("No host specified in server URL '{}'", input));
        }

        if let Some(port) = parsed.port() {
            self.server.port = port;
        } else if scheme_specified {
            match parsed.scheme() {
                "https" | "wss" => self.server.port = 443,
                "http" | "ws" => self.server.port = 80,
                _ => {}
            }
        }

        Ok(())
    }

    /// Process and normalize `url_override` if present in `[server]`.
    pub fn normalize(&mut self) -> Result<(), String> {
        if let Some(ref raw_url) = self.server.url_override.clone()
            && !raw_url.is_empty()
        {
            self.parse_and_apply_server_url(raw_url)?;
        }
        Ok(())
    }

    /// Helper to get the display host string (either server.host or formatted server.ip).
    pub fn host_str(&self) -> String {
        if let Some(ref host) = self.server.host {
            host.clone()
        } else {
            match self.server.ip {
                IpAddr::V4(ip) => ip.to_string(),
                IpAddr::V6(ip) => format!("[{}]", ip),
            }
        }
    }

    /// Construct full HTTP/HTTPS server base URL according to TLS settings, server host/IP, and port.
    pub fn server_url(&self) -> String {
        let host = self.host_str();
        let is_loopback = host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "[::1]"
            || self.server.ip.is_loopback();

        let scheme = if self.server.use_tls {
            "https"
        } else if is_loopback {
            "http"
        } else {
            tracing::warn!(
                "Insecure HTTP transport selected for non-loopback server {}",
                host
            );
            "http"
        };

        if (scheme == "http" && self.server.port == 80)
            || (scheme == "https" && self.server.port == 443)
        {
            format!("{}://{}", scheme, host)
        } else {
            format!("{}://{}:{}", scheme, host, self.server.port)
        }
    }

    /// Construct full WS/WSS WebSocket server URL according to TLS settings, server host/IP, and port.
    pub fn websocket_url(&self) -> String {
        let host = self.host_str();
        let is_loopback = host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "[::1]"
            || self.server.ip.is_loopback();

        let scheme = if self.server.use_tls {
            "wss"
        } else {
            if !is_loopback {
                tracing::warn!(
                    "Insecure WS transport selected for non-loopback server {}",
                    host
                );
            }
            "ws"
        };

        if (scheme == "ws" && self.server.port == 80)
            || (scheme == "wss" && self.server.port == 443)
        {
            format!("{}://{}", scheme, host)
        } else {
            format!("{}://{}:{}", scheme, host, self.server.port)
        }
    }

    /// Validate configuration settings.
    pub fn validate(&self) -> Result<(), String> {
        if self.server.port == 0 {
            return Err("Server port cannot be 0".to_string());
        }
        if self.audio.sample_rate < 8000 || self.audio.sample_rate > 96000 {
            return Err(format!(
                "Sample rate {} Hz out of valid range (8000..=96000 Hz)",
                self.audio.sample_rate
            ));
        }
        if self.audio.channels != 1 && self.audio.channels != 2 {
            return Err(format!(
                "Channels {} invalid (must be 1 for mono or 2 for stereo)",
                self.audio.channels
            ));
        }
        if self.network.stun_server.is_empty() {
            return Err("STUN server URL cannot be empty".to_string());
        }
        if !self.network.stun_server.starts_with("stun:")
            && !self.network.stun_server.starts_with("stuns:")
            && !self.network.stun_server.starts_with("turn:")
            && !self.network.stun_server.starts_with("turns:")
        {
            return Err(format!(
                "Invalid STUN/TURN URL format: '{}' (must start with stun:, stuns:, turn:, or turns:)",
                self.network.stun_server
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
        self.config.server.ip = ip;
        self
    }

    pub fn server_port(mut self, port: u16) -> Self {
        self.config.server.port = port;
        self
    }

    pub fn server_host(mut self, host: impl Into<String>) -> Self {
        self.config.server.host = Some(host.into());
        self
    }

    pub fn server_url(mut self, url: impl AsRef<str>) -> Result<Self, String> {
        self.config.parse_and_apply_server_url(url.as_ref())?;
        Ok(self)
    }

    pub fn stun_server(mut self, stun: impl Into<String>) -> Self {
        self.config.network.stun_server = stun.into();
        self
    }

    pub fn sample_rate(mut self, rate: u32) -> Self {
        self.config.audio.sample_rate = rate;
        self
    }

    pub fn channels(mut self, channels: u16) -> Self {
        self.config.audio.channels = channels;
        self
    }

    pub fn use_tls(mut self, use_tls: bool) -> Self {
        self.config.server.use_tls = use_tls;
        self
    }

    pub fn build(self) -> Result<Configuration, String> {
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_default() {
        let config = Configuration::default();
        assert_eq!(config.server.ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.server.port, 15000);
        assert_eq!(config.audio.sample_rate, 48000);
        assert_eq!(config.audio.channels, 1);
        assert!(!config.audio.allow_echoback);
        assert_eq!(config.network.stun_server, "stun:stun.l.google.com:19302");
    }

    #[test]
    fn test_configuration_serialization() {
        let config = Configuration::default();
        let toml_str = toml::to_string(&config).expect("Failed to serialize configuration");
        assert!(toml_str.contains("[server]"));
        assert!(toml_str.contains("[network]"));
        assert!(toml_str.contains("[audio]"));
        assert!(toml_str.contains("[client]"));
        assert!(toml_str.contains("ip"));
        assert!(toml_str.contains("port"));
        assert!(toml_str.contains("sample_rate"));
        assert!(toml_str.contains("channels"));
    }

    #[test]
    fn test_configuration_deserialization() {
        let toml_str = r#"
            [server]
            ip = "192.168.1.1"
            port = 16000

            [network]
            stun_server = "stun:stun.l.google.com:19302"

            [audio]
            sample_rate = 44100
            channels = 2
            allow_echoback = true
            input_device = "Shokz"
            output_device = "MacBook"
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.server.ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(config.server.port, 16000);
        assert_eq!(config.network.stun_server, "stun:stun.l.google.com:19302");
        assert_eq!(config.audio.sample_rate, 44100);
        assert_eq!(config.audio.channels, 2);
        assert!(config.audio.allow_echoback);
        assert_eq!(config.audio.input_device.as_deref(), Some("Shokz"));
        assert_eq!(config.audio.output_device.as_deref(), Some("MacBook"));
    }

    #[test]
    fn test_configuration_deserialization_with_server_url() {
        let toml_str = r#"
            [server]
            url = "http://192.168.1.50:16000"

            [audio]
            sample_rate = 48000
            channels = 1
        "#;
        let mut config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        config.normalize().expect("Normalize should succeed");
        assert_eq!(config.server_url(), "http://192.168.1.50:16000");
        assert_eq!(config.server.port, 16000);
        assert!(!config.server.use_tls);
        assert_eq!(config.server.ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)));
    }

    #[test]
    fn test_configuration_tls_scheme() {
        let mut config = Configuration::default();
        // Default loopback with use_tls = true -> https
        assert_eq!(config.server_url(), "https://127.0.0.1:15000");
        assert_eq!(config.websocket_url(), "wss://127.0.0.1:15000");

        // Non-loopback IP with use_tls = true -> https
        config.server.ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(config.server_url(), "https://192.168.1.100:15000");
        assert_eq!(config.websocket_url(), "wss://192.168.1.100:15000");

        // Non-loopback IP with use_tls = false -> http (insecure warning)
        config.server.use_tls = false;
        assert_eq!(config.server_url(), "http://192.168.1.100:15000");
        assert_eq!(config.websocket_url(), "ws://192.168.1.100:15000");
    }

    #[test]
    fn test_parse_and_apply_server_url() {
        let mut config = Configuration::default();

        // 1. Full HTTP URL
        config
            .parse_and_apply_server_url("http://192.168.1.50:16000")
            .unwrap();
        assert!(!config.server.use_tls);
        assert_eq!(config.server.port, 16000);
        assert_eq!(config.server.ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)));
        assert_eq!(config.server_url(), "http://192.168.1.50:16000");

        // 2. HTTPS Domain URL
        config
            .parse_and_apply_server_url("https://daemon.example.com:8443")
            .unwrap();
        assert!(config.server.use_tls);
        assert_eq!(config.server.port, 8443);
        assert_eq!(config.server.host.as_deref(), Some("daemon.example.com"));
        assert_eq!(config.server_url(), "https://daemon.example.com:8443");

        // 3. Localhost HTTP
        config
            .parse_and_apply_server_url("http://localhost:15000")
            .unwrap();
        assert!(!config.server.use_tls);
        assert_eq!(config.server.port, 15000);
        assert_eq!(config.server.ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.server_url(), "http://localhost:15000");

        // 4. IP with port (no scheme)
        config.parse_and_apply_server_url("10.0.0.5:17000").unwrap();
        assert_eq!(config.server.port, 17000);
        assert_eq!(config.server.ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)));

        // 5. Scheme default ports
        config
            .parse_and_apply_server_url("https://example.com")
            .unwrap();
        assert_eq!(config.server.port, 443);
        assert_eq!(config.server_url(), "https://example.com");

        config
            .parse_and_apply_server_url("http://example.com")
            .unwrap();
        assert_eq!(config.server.port, 80);
        assert_eq!(config.server_url(), "http://example.com");
    }

    #[test]
    fn test_configuration_validation() {
        let mut config = Configuration::default();
        assert!(config.validate().is_ok());

        config.server.port = 0;
        assert!(config.validate().is_err());

        config.server.port = 15000;
        config.audio.sample_rate = 500;
        assert!(config.validate().is_err());

        config.audio.sample_rate = 48000;
        config.audio.channels = 5;
        assert!(config.validate().is_err());

        config.audio.channels = 1;
        config.network.stun_server = "invalid_scheme".to_string();
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

        assert_eq!(config.server.port, 18000);
        assert_eq!(config.audio.sample_rate, 44100);
        assert_eq!(config.audio.channels, 2);
        assert_eq!(config.network.stun_server, "stun:stun.l.google.com:19302");

        let url_config = ConfigurationBuilder::new()
            .server_url("http://192.168.1.100:15000")
            .expect("URL parse should succeed")
            .build()
            .expect("Build should succeed");
        assert_eq!(url_config.server_url(), "http://192.168.1.100:15000");
    }
}
