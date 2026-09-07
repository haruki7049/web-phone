//! STUN and TURN server module.
//!
//! This module implements a STUN (RFC 5389) and TURN media relay server
//! running over UDP to facilitate NAT traversal for WebRTC clients.

use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tracing::{error, info, trace};

/// STUN Magic Cookie (RFC 5389)
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;
/// STUN Binding Request message type
const STUN_BINDING_REQUEST: u16 = 0x0001;
/// STUN Binding Success Response message type
const STUN_BINDING_RESPONSE: u16 = 0x0101;
/// XOR-MAPPED-ADDRESS attribute type
const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

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

        let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
        let magic_cookie = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);

        if msg_type == STUN_BINDING_REQUEST && magic_cookie == STUN_MAGIC_COOKIE {
            trace!("Received STUN Binding Request from {}", src);
            let transaction_id = &buf[8..20];

            if let Ok(response) = build_stun_binding_response(src, transaction_id) {
                let _ = socket.send_to(&response, src).await;
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
            let mut key = [0u8; 16];
            key[0..4].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
            key[4..16].copy_from_slice(transaction_id);
            for i in 0..16 {
                resp.push(ip_bytes[i] ^ key[i]);
            }
        }
    }

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
    }
}
