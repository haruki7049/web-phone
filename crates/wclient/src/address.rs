//! Address information module for identifying wclient users.
//!
//! This module provides the `UserAddress` struct and associated functionality
//! to represent and manage IPv6 address information used to recognize and
//! identify `wclient` users across the WebRTC audio network.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{AddrParseError, Ipv6Addr};
use std::str::FromStr;

/// Address information to recognize a `wclient` user using an IPv6 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserAddress {
    /// The IPv6 address of the wclient user.
    pub ip: Ipv6Addr,
}

impl UserAddress {
    /// Create a new `UserAddress` with the given IPv6 address.
    pub fn new(ip: Ipv6Addr) -> Self {
        Self { ip }
    }

    /// Create a `UserAddress` from raw 16-byte array representation of IPv6 address.
    pub fn from_octets(octets: [u8; 16]) -> Self {
        Self {
            ip: Ipv6Addr::from(octets),
        }
    }

    /// Return the raw 16-byte octets of the IPv6 address.
    pub fn octets(&self) -> [u8; 16] {
        self.ip.octets()
    }

    /// Generate a Unique Local IPv6 Address (ULA) in `fd00::/8` derived from a numeric client ID.
    pub fn generate_from_client_id(client_id: u64) -> Self {
        let bytes = client_id.to_be_bytes();
        let mut octets = [0u8; 16];
        octets[0] = 0xfd;
        octets[1] = 0x00;
        octets[8..16].copy_from_slice(&bytes);
        Self {
            ip: Ipv6Addr::from(octets),
        }
    }

    /// Check if the user IPv6 address is loopback (`::1`).
    pub fn is_loopback(&self) -> bool {
        self.ip.is_loopback()
    }

    /// Check if the user IPv6 address is unspecified (`::`).
    pub fn is_unspecified(&self) -> bool {
        self.ip.is_unspecified()
    }
}

impl Default for UserAddress {
    fn default() -> Self {
        Self {
            ip: Ipv6Addr::LOCALHOST,
        }
    }
}

impl fmt::Display for UserAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ip)
    }
}

impl FromStr for UserAddress {
    type Err = AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ip = s.parse::<Ipv6Addr>()?;
        Ok(Self { ip })
    }
}

impl From<Ipv6Addr> for UserAddress {
    fn from(ip: Ipv6Addr) -> Self {
        Self { ip }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_address_default() {
        let addr = UserAddress::default();
        assert_eq!(addr.ip, Ipv6Addr::LOCALHOST);
        assert!(addr.is_loopback());
        assert_eq!(addr.to_string(), "::1");
    }

    #[test]
    fn test_user_address_from_str() {
        let addr: UserAddress = "2001:db8::1".parse().expect("Failed to parse user address");
        assert_eq!(addr.ip, Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1));
        assert_eq!(addr.to_string(), "2001:db8::1");
    }

    #[test]
    fn test_user_address_octets() {
        let octets = [0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        let addr = UserAddress::from_octets(octets);
        assert_eq!(addr.octets(), octets);
        assert_eq!(addr.ip, Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1));
    }

    #[test]
    fn test_user_address_client_id_generation() {
        let addr = UserAddress::generate_from_client_id(42);
        let octets = addr.octets();
        assert_eq!(octets[0], 0xfd);
        assert_eq!(octets[1], 0x00);
        assert_eq!(u64::from_be_bytes(octets[8..16].try_into().unwrap()), 42);
    }

    #[test]
    fn test_user_address_serde() {
        let addr = UserAddress::new(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x1234));
        let json_str = serde_json::to_string(&addr).expect("Failed to serialize");
        let deserialized: UserAddress =
            serde_json::from_str(&json_str).expect("Failed to deserialize");
        assert_eq!(addr, deserialized);
    }
}
