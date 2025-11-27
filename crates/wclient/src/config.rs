//! Client configuration module.
//!
//! This module provides configuration types and defaults for the WebTransport
//! audio client.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

/// TLS certificate verification mode for connecting to the server.
///
/// This enum determines how the client verifies the server's TLS certificate
/// during the connection handshake.
///
/// # Example
///
/// ```
/// use wclient::config::TlsVerifyMode;
///
/// // For development with self-signed certificates
/// let dev_mode = TlsVerifyMode::Skip;
///
/// // For production with proper CA-signed certificates
/// let prod_mode = TlsVerifyMode::Native;
///
/// // For pinning specific certificate hashes
/// let pinned = TlsVerifyMode::CertificateHash {
///     hashes: vec!["abc123...".to_string()],
/// };
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TlsVerifyMode {
    /// Skip TLS certificate verification entirely.
    ///
    /// **WARNING**: This mode is insecure and should only be used for
    /// development with self-signed certificates. It allows connections
    /// to any server without validating the certificate.
    Skip,

    /// Use the system's native certificate store for verification.
    ///
    /// This is the recommended mode for production use. The client will
    /// verify the server's certificate against the certificates trusted
    /// by the operating system.
    #[default]
    Native,

    /// Verify using specific certificate SHA-256 hashes.
    ///
    /// This mode allows certificate pinning by specifying the expected
    /// SHA-256 hash(es) of the server's certificate. Useful for
    /// self-signed certificates in controlled environments.
    CertificateHash {
        /// List of acceptable SHA-256 certificate hashes.
        ///
        /// Supported formats:
        /// - Dotted hex format: `"ab:cd:ef:12:34:..."`
        /// - Bytes array format: `"[0xab, 0xcd, 0xef, 0x12, 0x34, ...]"`
        hashes: Vec<String>,
    },
}

/// Default path to the configuration file.
///
/// The configuration file is located at:
/// - Linux: `~/.config/web-phone-client/config.toml`
/// - macOS: `~/Library/Application Support/dev.haruki7049.web-phone-client/config.toml`
/// - Windows: `C:\Users\<user>\AppData\Roaming\haruki7049\web-phone-client\config\config.toml`
pub static DEFAULT_CONFIG_PATH: LazyLock<Mutex<PathBuf>> = LazyLock::new(|| {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "web-phone-client")
        .expect("Failed to search ProjectDirs for dev.haruki7049.web-phone-client");
    let mut result: PathBuf = proj_dirs.config_dir().to_path_buf();
    let filename: &str = "config.toml";

    result.push(filename);
    Mutex::new(result)
});

/// Global configuration instance.
///
/// This is initialized once at startup and provides read-only access
/// to the client configuration throughout the application.
pub static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

/// Client configuration for the WebTransport audio client.
///
/// This struct holds all configuration options for connecting to the
/// audio server and configuring audio capture/playback settings.
///
/// # Example
///
/// ```
/// use wclient::Configuration;
///
/// let config = Configuration::default();
/// assert_eq!(config.server_port, 15000);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Configuration {
    /// IP address of the audio server to connect to.
    pub server_ip: Ipv4Addr,
    /// Port number of the audio server.
    pub server_port: u16,
    /// Audio sample rate in Hz (e.g., 48000, 44100).
    pub sample_rate: u32,
    /// Number of audio channels (1 for mono, 2 for stereo).
    pub channels: u16,
    /// Allow echo back (hear your own voice).
    ///
    /// When `true`, the client will play back its own transmitted audio.
    /// When `false` (default), the client's own audio is filtered out.
    #[serde(default)]
    pub allow_echoback: bool,
    /// TLS certificate verification mode.
    ///
    /// Controls how the client verifies the server's TLS certificate.
    /// Defaults to `TlsVerifyMode::Skip` for backward compatibility.
    ///
    /// For production use, set to `TlsVerifyMode::Native` to use
    /// the system's certificate store.
    #[serde(default)]
    pub tls_verify: TlsVerifyMode,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            server_ip: Ipv4Addr::new(127, 0, 0, 1),
            server_port: 15000,
            sample_rate: 48000,
            channels: 1,
            allow_echoback: false,
            tls_verify: TlsVerifyMode::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuration_default() {
        let config = Configuration::default();
        assert_eq!(config.server_ip, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(config.server_port, 15000);
        assert_eq!(config.sample_rate, 48000);
        assert_eq!(config.channels, 1);
        assert!(!config.allow_echoback);
        assert_eq!(config.tls_verify, TlsVerifyMode::Skip);
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
            sample_rate = 44100
            channels = 2
            allow_echoback = true
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.server_ip, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(config.server_port, 16000);
        assert_eq!(config.sample_rate, 44100);
        assert_eq!(config.channels, 2);
        assert!(config.allow_echoback);
    }

    #[test]
    fn test_configuration_deserialization_with_defaults() {
        let toml_str = r#"
            server_ip = "127.0.0.1"
            server_port = 15000
            sample_rate = 48000
            channels = 1
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        // allow_echoback should default to false when not specified
        assert!(!config.allow_echoback);
        // tls_verify should default to Skip when not specified
        assert_eq!(config.tls_verify, TlsVerifyMode::Skip);
    }

    #[test]
    fn test_tls_verify_mode_skip() {
        let toml_str = r#"
            server_ip = "127.0.0.1"
            server_port = 15000
            sample_rate = 48000
            channels = 1
            [tls_verify]
            type = "skip"
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.tls_verify, TlsVerifyMode::Skip);
    }

    #[test]
    fn test_tls_verify_mode_native() {
        let toml_str = r#"
            server_ip = "127.0.0.1"
            server_port = 15000
            sample_rate = 48000
            channels = 1
            [tls_verify]
            type = "native"
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        assert_eq!(config.tls_verify, TlsVerifyMode::Native);
    }

    #[test]
    fn test_tls_verify_mode_certificate_hash() {
        let toml_str = r#"
            server_ip = "127.0.0.1"
            server_port = 15000
            sample_rate = 48000
            channels = 1
            [tls_verify]
            type = "certificate_hash"
            hashes = ["abc123", "def456"]
        "#;
        let config: Configuration = toml::from_str(toml_str).expect("Failed to deserialize");
        match config.tls_verify {
            TlsVerifyMode::CertificateHash { hashes } => {
                assert_eq!(hashes.len(), 2);
                assert_eq!(hashes[0], "abc123");
                assert_eq!(hashes[1], "def456");
            }
            _ => panic!("Expected CertificateHash mode"),
        }
    }

    #[test]
    fn test_tls_verify_mode_serialization() {
        let mode = TlsVerifyMode::Native;
        let toml_str = toml::to_string(&mode).expect("Failed to serialize");
        assert!(toml_str.contains("native"));

        let mode = TlsVerifyMode::CertificateHash {
            hashes: vec!["hash1".to_string()],
        };
        let toml_str = toml::to_string(&mode).expect("Failed to serialize");
        assert!(toml_str.contains("certificate_hash"));
        assert!(toml_str.contains("hash1"));
    }
}
