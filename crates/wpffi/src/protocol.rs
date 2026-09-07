//! WebRTC DataChannel packet protocol module.
//!
//! Provides type-safe encoding and decoding for web-phone DataChannel network packets.

use crate::address::UserAddress;
use std::fmt;

/// Errors that can occur during protocol packet decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// Packet data is empty.
    EmptyPacket,
    /// Unknown or unsupported packet header byte.
    UnknownPacketType(u8),
    /// Packet length is less than expected minimum.
    InsufficientLength {
        packet_type: u8,
        actual: usize,
        expected: usize,
    },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPacket => write!(f, "Packet data is empty"),
            Self::UnknownPacketType(t) => write!(f, "Unknown packet type: 0x{:02x}", t),
            Self::InsufficientLength {
                packet_type,
                actual,
                expected,
            } => {
                write!(
                    f,
                    "Insufficient packet length for type 0x{:02x}: got {} bytes, expected at least {}",
                    packet_type, actual, expected
                )
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

/// High-level, type-safe representation of DataChannel protocol messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolPacket {
    /// 0x00: Client Assignment [0x00, client_id (8b LE), user_address (32b SHA256)]
    ClientAssignment {
        client_id: u64,
        user_address: UserAddress,
    },
    /// 0x01: Broadcast Audio [0x01, sender_id (8b LE), audio_data...]
    BroadcastAudio { sender_id: u64, audio_data: Vec<u8> },
    /// 0x03: Targeted Audio (Client -> Server) [0x03, target_address (32b SHA256), audio_data...]
    ClientTargetedAudio {
        target_address: UserAddress,
        audio_data: Vec<u8>,
    },
    /// 0x03: Targeted Audio (Server -> Client) [0x03, target_address (32b), sender_id (8b LE), sender_address (32b), audio_data...]
    ServerTargetedAudio {
        target_address: UserAddress,
        sender_id: u64,
        sender_address: UserAddress,
        audio_data: Vec<u8>,
    },
    /// 0x04: Call Request Notification [0x04, caller_id (8b LE), caller_address (32b SHA256)]
    CallRequest {
        caller_id: u64,
        caller_address: UserAddress,
    },
    /// 0x05: Call Accept Response [0x05, caller_address (32b SHA256)]
    CallAcceptResponse { caller_address: UserAddress },
    /// 0x06: Call Reject Response [0x06, caller_address (32b SHA256)]
    CallRejectResponse { caller_address: UserAddress },
    /// 0x07: Call Rejected Notification [0x07, target_address (32b SHA256)]
    CallRejectedNotification { target_address: UserAddress },
    /// 0x08: Call Accepted Notification [0x08, target_address (32b SHA256)]
    CallAcceptedNotification { target_address: UserAddress },
    /// 0xFF: Connection Error / Rejection [0xFF, target_address (32b SHA256)]
    ConnectionError { target_address: UserAddress },
}

impl ProtocolPacket {
    /// Safely decode raw DataChannel byte slice into a `ProtocolPacket`.
    pub fn decode(data: &[u8]) -> Result<Self, ProtocolError> {
        if data.is_empty() {
            return Err(ProtocolError::EmptyPacket);
        }

        let msg_type = data[0];
        match msg_type {
            0x00 => {
                if data.len() < 9 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x00,
                        actual: data.len(),
                        expected: 9,
                    });
                }
                let client_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
                let user_address = if data.len() >= 41 {
                    let bytes: [u8; 32] = data[9..41].try_into().unwrap();
                    UserAddress::from_bytes(bytes)
                } else {
                    UserAddress::default()
                };
                Ok(Self::ClientAssignment {
                    client_id,
                    user_address,
                })
            }
            0x01 => {
                if data.len() < 9 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x01,
                        actual: data.len(),
                        expected: 9,
                    });
                }
                let sender_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
                let audio_data = data[9..].to_vec();
                Ok(Self::BroadcastAudio {
                    sender_id,
                    audio_data,
                })
            }
            0x03 => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x03,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let target_bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let target_address = UserAddress::from_bytes(target_bytes);

                if data.len() >= 73 {
                    let sender_id = u64::from_le_bytes(data[33..41].try_into().unwrap());
                    let sender_bytes: [u8; 32] = data[41..73].try_into().unwrap();
                    let sender_address = UserAddress::from_bytes(sender_bytes);
                    let audio_data = data[73..].to_vec();

                    Ok(Self::ServerTargetedAudio {
                        target_address,
                        sender_id,
                        sender_address,
                        audio_data,
                    })
                } else {
                    let audio_data = data[33..].to_vec();
                    Ok(Self::ClientTargetedAudio {
                        target_address,
                        audio_data,
                    })
                }
            }
            0x04 => {
                if data.len() < 41 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x04,
                        actual: data.len(),
                        expected: 41,
                    });
                }
                let caller_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
                let caller_bytes: [u8; 32] = data[9..41].try_into().unwrap();
                let caller_address = UserAddress::from_bytes(caller_bytes);
                Ok(Self::CallRequest {
                    caller_id,
                    caller_address,
                })
            }
            0x05 => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x05,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let caller_bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let caller_address = UserAddress::from_bytes(caller_bytes);
                Ok(Self::CallAcceptResponse { caller_address })
            }
            0x06 => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x06,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let caller_bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let caller_address = UserAddress::from_bytes(caller_bytes);
                Ok(Self::CallRejectResponse { caller_address })
            }
            0x07 => {
                let target_address = if data.len() >= 33 {
                    let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                    UserAddress::from_bytes(bytes)
                } else {
                    UserAddress::default()
                };
                Ok(Self::CallRejectedNotification { target_address })
            }
            0x08 => {
                let target_address = if data.len() >= 33 {
                    let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                    UserAddress::from_bytes(bytes)
                } else {
                    UserAddress::default()
                };
                Ok(Self::CallAcceptedNotification { target_address })
            }
            0xFF => {
                let target_address = if data.len() >= 33 {
                    let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                    UserAddress::from_bytes(bytes)
                } else {
                    UserAddress::default()
                };
                Ok(Self::ConnectionError { target_address })
            }
            unknown => Err(ProtocolError::UnknownPacketType(unknown)),
        }
    }

    /// Encode `ProtocolPacket` into raw byte vector for DataChannel transmission.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::ClientAssignment {
                client_id,
                user_address,
            } => {
                let mut buf = Vec::with_capacity(41);
                buf.push(0x00);
                buf.extend_from_slice(&client_id.to_le_bytes());
                buf.extend_from_slice(&user_address.to_bytes());
                buf
            }
            Self::BroadcastAudio {
                sender_id,
                audio_data,
            } => {
                let mut buf = Vec::with_capacity(9 + audio_data.len());
                buf.push(0x01);
                buf.extend_from_slice(&sender_id.to_le_bytes());
                buf.extend_from_slice(audio_data);
                buf
            }
            Self::ClientTargetedAudio {
                target_address,
                audio_data,
            } => {
                let mut buf = Vec::with_capacity(33 + audio_data.len());
                buf.push(0x03);
                buf.extend_from_slice(&target_address.to_bytes());
                buf.extend_from_slice(audio_data);
                buf
            }
            Self::ServerTargetedAudio {
                target_address,
                sender_id,
                sender_address,
                audio_data,
            } => {
                let mut buf = Vec::with_capacity(73 + audio_data.len());
                buf.push(0x03);
                buf.extend_from_slice(&target_address.to_bytes());
                buf.extend_from_slice(&sender_id.to_le_bytes());
                buf.extend_from_slice(&sender_address.to_bytes());
                buf.extend_from_slice(audio_data);
                buf
            }
            Self::CallRequest {
                caller_id,
                caller_address,
            } => {
                let mut buf = Vec::with_capacity(41);
                buf.push(0x04);
                buf.extend_from_slice(&caller_id.to_le_bytes());
                buf.extend_from_slice(&caller_address.to_bytes());
                buf
            }
            Self::CallAcceptResponse { caller_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x05);
                buf.extend_from_slice(&caller_address.to_bytes());
                buf
            }
            Self::CallRejectResponse { caller_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x06);
                buf.extend_from_slice(&caller_address.to_bytes());
                buf
            }
            Self::CallRejectedNotification { target_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x07);
                buf.extend_from_slice(&target_address.to_bytes());
                buf
            }
            Self::CallAcceptedNotification { target_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x08);
                buf.extend_from_slice(&target_address.to_bytes());
                buf
            }
            Self::ConnectionError { target_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0xFF);
                buf.extend_from_slice(&target_address.to_bytes());
                buf
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_assignment_roundtrip() {
        let addr = UserAddress::generate_from_time();
        let packet = ProtocolPacket::ClientAssignment {
            client_id: 12345,
            user_address: addr.clone(),
        };

        let bytes = packet.encode();
        assert_eq!(bytes.len(), 41);
        assert_eq!(bytes[0], 0x00);

        let decoded = ProtocolPacket::decode(&bytes).expect("Failed to decode ClientAssignment");
        assert_eq!(decoded, packet);
    }

    #[test]
    fn test_broadcast_audio_roundtrip() {
        let packet = ProtocolPacket::BroadcastAudio {
            sender_id: 42,
            audio_data: vec![1, 2, 3, 4, 5],
        };

        let bytes = packet.encode();
        assert_eq!(bytes[0], 0x01);

        let decoded = ProtocolPacket::decode(&bytes).expect("Failed to decode BroadcastAudio");
        assert_eq!(decoded, packet);
    }

    #[test]
    fn test_targeted_audio_roundtrips() {
        let target = UserAddress::generate_from_time();
        let sender = UserAddress::generate_from_time();

        let client_packet = ProtocolPacket::ClientTargetedAudio {
            target_address: target.clone(),
            audio_data: vec![10, 20, 30],
        };
        let bytes = client_packet.encode();
        assert_eq!(bytes[0], 0x03);
        let decoded = ProtocolPacket::decode(&bytes).expect("Failed to decode ClientTargetedAudio");
        assert_eq!(decoded, client_packet);

        let server_packet = ProtocolPacket::ServerTargetedAudio {
            target_address: target.clone(),
            sender_id: 100,
            sender_address: sender.clone(),
            audio_data: vec![10, 20, 30],
        };
        let server_bytes = server_packet.encode();
        assert_eq!(server_bytes[0], 0x03);
        let server_decoded =
            ProtocolPacket::decode(&server_bytes).expect("Failed to decode ServerTargetedAudio");
        assert_eq!(server_decoded, server_packet);
    }

    #[test]
    fn test_call_control_roundtrips() {
        let addr = UserAddress::generate_from_time();

        let req = ProtocolPacket::CallRequest {
            caller_id: 99,
            caller_address: addr.clone(),
        };
        assert_eq!(ProtocolPacket::decode(&req.encode()).unwrap(), req);

        let accept_resp = ProtocolPacket::CallAcceptResponse {
            caller_address: addr.clone(),
        };
        assert_eq!(
            ProtocolPacket::decode(&accept_resp.encode()).unwrap(),
            accept_resp
        );

        let reject_resp = ProtocolPacket::CallRejectResponse {
            caller_address: addr.clone(),
        };
        assert_eq!(
            ProtocolPacket::decode(&reject_resp.encode()).unwrap(),
            reject_resp
        );

        let rejected_notif = ProtocolPacket::CallRejectedNotification {
            target_address: addr.clone(),
        };
        assert_eq!(
            ProtocolPacket::decode(&rejected_notif.encode()).unwrap(),
            rejected_notif
        );

        let accepted_notif = ProtocolPacket::CallAcceptedNotification {
            target_address: addr.clone(),
        };
        assert_eq!(
            ProtocolPacket::decode(&accepted_notif.encode()).unwrap(),
            accepted_notif
        );

        let conn_err = ProtocolPacket::ConnectionError {
            target_address: addr.clone(),
        };
        assert_eq!(
            ProtocolPacket::decode(&conn_err.encode()).unwrap(),
            conn_err
        );
    }

    #[test]
    fn test_decode_errors() {
        assert_eq!(ProtocolPacket::decode(&[]), Err(ProtocolError::EmptyPacket));
        assert_eq!(
            ProtocolPacket::decode(&[0x99]),
            Err(ProtocolError::UnknownPacketType(0x99))
        );
        assert_eq!(
            ProtocolPacket::decode(&[0x00, 1, 2, 3]),
            Err(ProtocolError::InsufficientLength {
                packet_type: 0x00,
                actual: 4,
                expected: 9
            })
        );
    }
}
