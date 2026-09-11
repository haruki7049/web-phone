//! STUN and TURN server module.
//!
//! This module implements a STUN (RFC 5389) and TURN media relay server
//! running over UDP to facilitate NAT traversal for WebRTC clients.

use crate::rate_limit::RateLimiter;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};
use tokio::net::UdpSocket;
use tracing::{error, info, trace, warn};
use wpapi::{AuthError, UserAddress, verify_ephemeral_turn_credential};

/// Verify incoming TURN Allocation credentials against configured server secret (WPIP-10).
pub fn verify_turn_allocation_credentials(
    username: &str,
    credential: &str,
) -> Result<UserAddress, AuthError> {
    let secret_bytes = crate::config::get_turn_server_secret();
    verify_ephemeral_turn_credential(&secret_bytes, username, credential)
}

use crate::constants::{STUN_RATE_LIMIT_BURST, STUN_RATE_LIMIT_REFILL};

/// Static STUN IP rate limiter: 20 requests/sec, max burst of 50.
static STUN_RATE_LIMITER: LazyLock<Arc<RateLimiter>> = LazyLock::new(|| {
    Arc::new(RateLimiter::new(
        STUN_RATE_LIMIT_REFILL,
        STUN_RATE_LIMIT_BURST,
    ))
});

/// STUN Magic Cookie (RFC 5389)
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;
/// STUN Binding Request message type
const STUN_BINDING_REQUEST: u16 = 0x0001;
/// STUN Binding Success Response message type
const STUN_BINDING_RESPONSE: u16 = 0x0101;
/// TURN Allocate Request message type (WPIP-10 / RFC 5766)
const TURN_ALLOCATE_REQUEST: u16 = 0x0003;
/// TURN Allocate Error Response message type (WPIP-10)
const TURN_ALLOCATE_ERROR_RESPONSE: u16 = 0x0113;
/// XOR-MAPPED-ADDRESS attribute type
const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
/// STUN Attribute USERNAME (0x0006)
const STUN_ATTR_USERNAME: u16 = 0x0006;
/// STUN Attribute MESSAGE-INTEGRITY (0x0008)
const STUN_ATTR_MESSAGE_INTEGRITY: u16 = 0x0008;

/// Start the STUN/TURN UDP server service.
pub async fn run_stun_server(
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let socket = UdpSocket::bind(addr).await?;
    info!("STUN/TURN server listening on UDP {}", addr);

    let mut buf = vec![0u8; 4096];

    loop {
        let (len, src) = match socket.recv_from(&mut buf).await {
            Ok(res) => res,
            Err(e) => {
                error!("STUN/TURN socket error: {}", e);
                continue;
            }
        };

        if len < 20 {
            continue; // Not a valid STUN header
        }

        if !STUN_RATE_LIMITER.check_and_consume(src.ip()) {
            warn!(
                "STUN packet rate limit exceeded for IP: {}, dropping",
                crate::rate_limit::sanitize_ip(&src.ip().to_string())
            );
            continue;
        }

        let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
        let magic_cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);

        if msg_type == STUN_BINDING_REQUEST && magic_cookie == STUN_MAGIC_COOKIE {
            trace!(
                "Received STUN Binding Request from {}",
                crate::rate_limit::sanitize_ip(&src.to_string())
            );
            let transaction_id = &buf[8..20];

            if let Ok(response) = build_stun_binding_response(src, transaction_id) {
                let _ = socket.send_to(&response, src).await;
            }
        } else if msg_type == TURN_ALLOCATE_REQUEST && magic_cookie == STUN_MAGIC_COOKIE {
            let transaction_id = &buf[8..20];
            let (username_opt, credential_opt) = parse_stun_attributes(&buf[..len]);
            let is_valid = match (username_opt, credential_opt) {
                (Some(u), Some(c)) => verify_turn_allocation_credentials(&u, &c).is_ok(),
                (Some(u), None) => verify_turn_allocation_credentials(&u, "").is_ok(),
                _ => false,
            };

            if !is_valid {
                warn!(
                    "TURN Allocate request authentication failed for IP: {}",
                    crate::rate_limit::sanitize_ip(&src.ip().to_string())
                );
                if let Ok(err_resp) =
                    build_turn_allocate_error_response(transaction_id, 401, "Unauthorized")
                {
                    let _ = socket.send_to(&err_resp, src).await;
                }
            } else {
                info!(
                    "TURN Allocate request authenticated successfully for IP: {}",
                    crate::rate_limit::sanitize_ip(&src.ip().to_string())
                );
                if let Ok(response) = build_stun_binding_response(src, transaction_id) {
                    let _ = socket.send_to(&response, src).await;
                }
            }
        }
    }
}

/// Construct a STUN Binding Success Response with XOR-MAPPED-ADDRESS (IPv4 and IPv6).
fn build_stun_binding_response(
    src: SocketAddr,
    transaction_id: &[u8],
) -> Result<Vec<u8>, &'static str> {
    if transaction_id.len() != 12 {
        return Err("Transaction ID must be 12 bytes");
    }

    let mut resp = Vec::with_capacity(40);

    // Message Type: 0x0101 (Binding Response)
    resp.extend_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());

    let (family, value_len, attr_len) = match src {
        SocketAddr::V4(_) => (0x01u8, 8u16, 12u16),
        SocketAddr::V6(_) => (0x02u8, 20u16, 24u16),
    };

    // Attribute length
    resp.extend_from_slice(&attr_len.to_be_bytes());

    // Magic Cookie
    resp.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());

    // Transaction ID (12 bytes)
    resp.extend_from_slice(transaction_id);

    // XOR-MAPPED-ADDRESS Attribute (0x0020)
    resp.extend_from_slice(&STUN_ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    resp.extend_from_slice(&value_len.to_be_bytes());

    // Reserved (1 byte: 0x00), Family (1 byte: 0x01 for IPv4, 0x02 for IPv6)
    resp.push(0x00);
    resp.push(family);

    // XOR-ed Port
    let port = src.port();
    let xor_port = port ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
    resp.extend_from_slice(&xor_port.to_be_bytes());

    match src {
        SocketAddr::V4(addr) => {
            let ip_bytes = addr.ip().octets();
            let magic_cookie_bytes = STUN_MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                resp.push(ip_bytes[i] ^ magic_cookie_bytes[i]);
            }
        }
        SocketAddr::V6(addr) => {
            let ip_bytes = addr.ip().octets();
            let mut xor_mask = [0u8; 16];
            xor_mask[0..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
            xor_mask[4..16].copy_from_slice(transaction_id);
            for i in 0..16 {
                resp.push(ip_bytes[i] ^ xor_mask[i]);
            }
        }
    }

    Ok(resp)
}

fn parse_stun_attributes(data: &[u8]) -> (Option<String>, Option<String>) {
    if data.len() < 20 {
        return (None, None);
    }
    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let end = (20 + msg_len).min(data.len());
    let mut offset = 20;
    let mut username = None;
    let mut credential = None;

    while offset + 4 <= end {
        let attr_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let attr_len = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
        offset += 4;
        if offset + attr_len > end {
            break;
        }
        let val_bytes = &data[offset..offset + attr_len];
        if attr_type == STUN_ATTR_USERNAME {
            if let Ok(s) = std::str::from_utf8(val_bytes) {
                username = Some(s.to_string());
            }
        } else if attr_type == STUN_ATTR_MESSAGE_INTEGRITY {
            use base64::Engine;
            let cred = base64::engine::general_purpose::STANDARD.encode(val_bytes);
            credential = Some(cred);
        }
        let padded_len = (attr_len + 3) & !3;
        offset += padded_len;
    }
    (username, credential)
}

fn build_turn_allocate_error_response(
    transaction_id: &[u8],
    error_code: u16,
    reason: &str,
) -> Result<Vec<u8>, &'static str> {
    if transaction_id.len() != 12 {
        return Err("Transaction ID must be 12 bytes");
    }
    let mut resp = Vec::with_capacity(32 + reason.len());
    resp.extend_from_slice(&TURN_ALLOCATE_ERROR_RESPONSE.to_be_bytes());
    let attr_len = 4 + reason.len() as u16;
    resp.extend_from_slice(&(attr_len + 4).to_be_bytes());
    resp.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    resp.extend_from_slice(transaction_id);

    // ERROR-CODE attribute (0x0009)
    resp.extend_from_slice(&0x0009u16.to_be_bytes());
    resp.extend_from_slice(&attr_len.to_be_bytes());
    resp.push(0x00);
    resp.push(0x00);
    resp.push((error_code / 100) as u8);
    resp.push((error_code % 100) as u8);
    resp.extend_from_slice(reason.as_bytes());

    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{SocketAddrV4, SocketAddrV6};

    #[test]
    fn test_build_stun_binding_response() {
        let src = SocketAddr::V4(SocketAddrV4::new("127.0.0.1".parse().unwrap(), 12345));
        let transaction_id = [1u8; 12];
        let resp =
            build_stun_binding_response(src, &transaction_id).expect("Failed to build response");

        assert_eq!(&resp[0..2], &STUN_BINDING_RESPONSE.to_be_bytes());
        assert_eq!(&resp[4..8], &STUN_MAGIC_COOKIE.to_be_bytes());
        assert_eq!(&resp[8..20], &transaction_id);
    }

    #[test]
    fn test_webrtc_stun_unmarshal() {
        use rtc_stun::message::Getter;
        use rtc_stun::message::Message;
        use rtc_stun::xoraddr::XorMappedAddress;
        let src = SocketAddr::V4(SocketAddrV4::new("127.0.0.1".parse().unwrap(), 12345));
        let transaction_id = [1u8; 12];
        let resp =
            build_stun_binding_response(src, &transaction_id).expect("Failed to build response");
        let mut msg = Message::new();
        msg.unmarshal_binary(&resp)
            .expect("Failed to unmarshal binary STUN response");
        let mut xor_addr = XorMappedAddress::default();
        xor_addr
            .get_from(&msg)
            .expect("Failed to get XorMappedAddress from message");
    }

    #[test]
    fn test_build_stun_binding_response_v6() {
        let src = SocketAddr::V6(SocketAddrV6::new(
            "2001:db8::1".parse().unwrap(),
            12345,
            0,
            0,
        ));
        let transaction_id = [1u8; 12];
        let resp = build_stun_binding_response(src, &transaction_id)
            .expect("Failed to build IPv6 response");

        assert_eq!(&resp[0..2], &STUN_BINDING_RESPONSE.to_be_bytes());
        assert_eq!(&resp[4..8], &STUN_MAGIC_COOKIE.to_be_bytes());
        assert_eq!(&resp[8..20], &transaction_id);
        // Verify family byte for IPv6
        assert_eq!(resp[25], 0x02);

        // Verify IPv6 address XOR unmasking
        let mut xor_mask = [0u8; 16];
        xor_mask[0..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        xor_mask[4..16].copy_from_slice(&transaction_id);

        let xored_ip = &resp[28..44];
        let mut unmasked_ip = [0u8; 16];
        for i in 0..16 {
            unmasked_ip[i] = xored_ip[i] ^ xor_mask[i];
        }
        let expected_ip: std::net::Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert_eq!(unmasked_ip, expected_ip.octets());
    }

    #[test]
    fn test_build_stun_binding_response_invalid_transaction_id() {
        let src = SocketAddr::V4(SocketAddrV4::new("127.0.0.1".parse().unwrap(), 12345));
        let short_id = [1u8; 10]; // 10 bytes instead of 12
        let result = build_stun_binding_response(src, &short_id);
        assert!(result.is_err());
    }
}
