//! Server configuration module.
//!
//! This module provides configuration types and defaults for the
//! WebTransport audio server daemon.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

/// Default path to the configuration file.
///
/// The configuration file is located at:
/// - Linux: `~/.config/web-phone-daemon/config.toml`
/// - macOS: `~/Library/Application Support/dev.haruki7049.web-phone-daemon/config.toml`
/// - Windows: `C:\Users\<user>\AppData\Roaming\haruki7049\web-phone-daemon\config\config.toml`
pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-daemon")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-daemon");
    let mut config_path: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    config_path.push(filename);
    Mutex::new(config_path)
});

/// Global configuration instance.
///
/// This is initialized once at startup and provides read-only access
/// to the server configuration throughout the application.
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Server configuration for the WebTransport audio daemon.
///
/// This struct holds all configuration options for binding the server
/// and managing audio transmission.
///
/// # Example
///
/// ```
/// use wdaemon::Configuration;
///
/// let config = Configuration::default();
/// assert_eq!(config.port, 15000);
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    /// IP address to bind the server to.
    ///
    /// Use `0.0.0.0` to listen on all interfaces, or a specific IP
    /// to bind to a particular interface.
    pub ip: Ipv4Addr,
    /// Port number to listen on.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_default() {
        let config = Configuration::default();
        assert_eq!(config.ip, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(config.port, 15000);
    }

    #[test]
    fn test_configuration_serialization() {
        let config = Configuration::default();
        let toml_str = toml::to_string(&config).expect("Failed to serialize configuration");
        assert!(toml_str.contains("ip"));
        assert!(toml_str.contains("port"));
    }

    #[test]
    fn test_configuration_deserialization() {
        let toml_str = r#"
            ip = "192.168.1.1"
            port = 16000
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.ip, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(config.port, 16000);
    }

    #[test]
    fn test_configuration_custom_port() {
        let toml_str = r#"
            ip = "0.0.0.0"
            port = 8080
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.ip, Ipv4Addr::new(0, 0, 0, 0));
        assert_eq!(config.port, 8080);
    }
}
