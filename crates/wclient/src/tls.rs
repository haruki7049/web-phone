//! TLS configuration and verification utilities.
//!
//! This module provides utilities for configuring TLS connections,
//! including SPKI (Subject Public Key Info) pinning for certificate
//! verification.

use anyhow::{Result, anyhow};
use ring::digest::{SHA256, digest};
use x509_parser::prelude::*;

/// SPKI SHA-256 hash (32 bytes).
pub type SpkiSha256 = [u8; 32];

/// Calculate the SPKI SHA-256 hash from a DER-encoded certificate.
///
/// This function parses an X.509 certificate and extracts the Subject Public
/// Key Info (SPKI), then calculates its SHA-256 hash. This hash can be used
/// for certificate pinning that survives certificate renewals (as long as
/// the public key remains the same).
///
/// # Arguments
///
/// * `cert_der` - DER-encoded X.509 certificate bytes
///
/// # Returns
///
/// Returns the 32-byte SHA-256 hash of the certificate's SPKI, or an error
/// if the certificate cannot be parsed.
///
/// # Example
///
/// ```ignore
/// let cert_der = std::fs::read("cert.der")?;
/// let spki_hash = calculate_spki_sha256(&cert_der)?;
/// println!("SPKI SHA-256: {:?}", spki_hash);
/// ```
pub fn calculate_spki_sha256(cert_der: &[u8]) -> Result<SpkiSha256> {
    let (_, cert) = X509Certificate::from_der(cert_der)
        .map_err(|e| anyhow!("Failed to parse X.509 certificate: {:?}", e))?;

    // Get the raw SPKI bytes from the certificate
    let spki_raw = cert.public_key().raw;

    // Calculate SHA-256 hash of the SPKI
    let hash = digest(&SHA256, spki_raw);
    let mut result = [0u8; 32];
    result.copy_from_slice(hash.as_ref());

    Ok(result)
}

/// Parse a hex string (with optional colons) into a 32-byte hash.
///
/// Supports formats:
/// - Dotted hex: `"ab:cd:ef:12:34:..."`
/// - Plain hex: `"abcdef1234..."`
///
/// # Arguments
///
/// * `s` - Hex string to parse
///
/// # Returns
///
/// Returns the 32-byte hash or an error if the format is invalid.
pub fn parse_hex_hash(s: &str) -> Result<SpkiSha256> {
    // Remove colons if present
    let clean: String = s.chars().filter(|c| *c != ':').collect();

    if clean.len() != 64 {
        return Err(anyhow!(
            "Invalid hash length: expected 64 hex characters (32 bytes), got {}",
            clean.len()
        ));
    }

    let bytes: Result<Vec<u8>, _> = (0..32)
        .map(|i| u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16))
        .collect();

    let bytes = bytes.map_err(|e| anyhow!("Invalid hex character: {}", e))?;

    let mut result = [0u8; 32];
    result.copy_from_slice(&bytes);
    Ok(result)
}

/// Format a 32-byte hash as a dotted hex string.
///
/// # Arguments
///
/// * `hash` - 32-byte hash to format
///
/// # Returns
///
/// Returns the hash formatted as `"ab:cd:ef:..."`.
pub fn format_hash_dotted(hash: &SpkiSha256) -> String {
    hash.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(":")
}

/// Verify that a certificate's SPKI hash matches one of the allowed hashes.
///
/// # Arguments
///
/// * `cert_der` - DER-encoded X.509 certificate bytes
/// * `allowed_hashes` - List of allowed SPKI SHA-256 hashes
///
/// # Returns
///
/// Returns `Ok(())` if the certificate's SPKI hash matches one of the allowed
/// hashes, or an error otherwise.
pub fn verify_spki_hash(cert_der: &[u8], allowed_hashes: &[SpkiSha256]) -> Result<()> {
    let actual_hash = calculate_spki_sha256(cert_der)?;

    if allowed_hashes.contains(&actual_hash) {
        Ok(())
    } else {
        Err(anyhow!(
            "Certificate SPKI hash {} does not match any allowed hashes",
            format_hash_dotted(&actual_hash)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_hash_dotted() {
        let hash_str = "ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89";
        let result = parse_hex_hash(hash_str).unwrap();
        assert_eq!(result[0], 0xab);
        assert_eq!(result[1], 0xcd);
        assert_eq!(result[31], 0x89);
    }

    #[test]
    fn test_parse_hex_hash_plain() {
        let hash_str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let result = parse_hex_hash(hash_str).unwrap();
        assert_eq!(result[0], 0xab);
        assert_eq!(result[1], 0xcd);
    }

    #[test]
    fn test_parse_hex_hash_invalid_length() {
        let hash_str = "abcd";
        assert!(parse_hex_hash(hash_str).is_err());
    }

    #[test]
    fn test_parse_hex_hash_invalid_chars() {
        let hash_str = "ghij0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        assert!(parse_hex_hash(hash_str).is_err());
    }

    #[test]
    fn test_format_hash_dotted() {
        let hash = [0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89,
                    0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89,
                    0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89,
                    0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89];
        let formatted = format_hash_dotted(&hash);
        assert!(formatted.starts_with("ab:cd:ef:01"));
        assert_eq!(formatted.len(), 95); // 32 * 2 + 31 colons
    }
}
