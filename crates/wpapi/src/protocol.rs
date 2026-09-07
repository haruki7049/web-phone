//! WebRTC DataChannel packet protocol module.
//!
//! Provides type-safe encoding and decoding for web-phone DataChannel network packets
//! strictly conforming to WPIP specifications (WPIP-01 through WPIP-12).

use crate::address::UserAddress;
use std::fmt;

/// Default Opus Codec ID as defined in WPIP-04.
pub const CODEC_OPUS: u8 = 0x01;
/// PCM 32-bit float LE Codec ID.
pub const CODEC_PCM_F32LE: u8 = 0x00;
/// PCM 16-bit signed integer LE Codec ID.
pub const CODEC_PCM_S16LE: u8 = 0x02;

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
    /// 0x01: Client Assignment [0x01, client_id (8b LE), user_address (32b raw)]
    ClientAssignment {
        client_id: u64,
        user_address: UserAddress,
    },
    /// 0x02: Client Targeted Audio [0x02, target_address (32b raw), codec_id (1b), audio_data...]
    ClientTargetedAudio {
        target_address: UserAddress,
        codec_id: u8,
        audio_data: Vec<u8>,
    },
    /// 0x03: Server Targeted Audio [0x03, target_address (32b raw), sender_id (8b LE), sender_address (32b raw), codec_id (1b), audio_data...]
    ServerTargetedAudio {
        target_address: UserAddress,
        sender_id: u64,
        sender_address: UserAddress,
        codec_id: u8,
        audio_data: Vec<u8>,
    },
    /// 0x04: Peer Targeted Audio [0x04, sender_id (8b LE), origin_node (8b LE), target_address (32b raw), sender_address (32b raw), codec_id (1b), ttl (1b), audio_data...]
    PeerTargetedAudio {
        sender_id: u64,
        origin_node: u64,
        target_address: UserAddress,
        sender_address: UserAddress,
        codec_id: u8,
        ttl: u8,
        audio_data: Vec<u8>,
    },
    /// 0x05: Call Request Notification [0x05, caller_id (8b LE), caller_address (32b raw)]
    CallRequest {
        caller_id: u64,
        caller_address: UserAddress,
    },
    /// 0x06: Call Accept Response [0x06, caller_address (32b raw)]
    CallAcceptResponse { caller_address: UserAddress },
    /// 0x07: Call Reject Response [0x07, caller_address (32b raw)]
    CallRejectResponse { caller_address: UserAddress },
    /// 0x08: Call Accepted Notification [0x08, target_address (32b raw)]
    CallAcceptedNotification { target_address: UserAddress },
    /// 0x09: Call Rejected Notification [0x09, target_address (32b raw)]
    CallRejectedNotification { target_address: UserAddress },
    /// 0x0A: Connection Error / Rejection [0x0A, target_address (32b raw)]
    ConnectionError { target_address: UserAddress },
    /// 0x0B: Call Hangup [0x0B, target_address (32b raw)]
    CallHangup { target_address: UserAddress },
    /// 0x0C: Call Ended Notification [0x0C, target_address (32b raw)]
    CallEndedNotification { target_address: UserAddress },
    /// 0x0D: Room Join Request [0x0D, room_address (32b raw)]
    RoomJoinRequest { room_address: UserAddress },
    /// 0x0E: Room State Notification [0x0E, room_address (32b raw), participant_count (4b LE)]
    RoomStateNotification {
        room_address: UserAddress,
        participant_count: u32,
    },
    /// 0x0F: Room Leave Request [0x0F, room_address (32b raw)]
    RoomLeaveRequest { room_address: UserAddress },
    /// 0x10: Room Group Audio [0x10, room_address (32b raw), codec_id (1b), audio_data...]
    RoomGroupAudio {
        room_address: UserAddress,
        codec_id: u8,
        audio_data: Vec<u8>,
    },
    /// 0x11: Active Speaker Notice [0x11, room_address (32b raw), speaker_addresses...]
    ActiveSpeakerNotice {
        room_address: UserAddress,
        speaker_addresses: Vec<UserAddress>,
    },
    /// 0x12: Ping [0x12, timestamp (8b LE)]
    Ping { timestamp: u64 },
    /// 0x13: Pong [0x13, timestamp (8b LE)]
    Pong { timestamp: u64 },
    /// Legacy Broadcast Audio [Broadcast tag, sender_id (8b LE), audio_data...]
    BroadcastAudio { sender_id: u64, audio_data: Vec<u8> },
}

impl ProtocolPacket {
    /// Safely decode raw DataChannel byte slice into a `ProtocolPacket`.
    pub fn decode(data: &[u8]) -> Result<Self, ProtocolError> {
        if data.is_empty() {
            return Err(ProtocolError::EmptyPacket);
        }

        let msg_type = data[0];
        match msg_type {
            // 0x01: ClientAssignment (or legacy 0x00)
            0x00 | 0x01 => {
                if msg_type == 0x00 && data.len() >= 9 && data.len() < 41 {
                    // Legacy Broadcast Audio fallback
                    let sender_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
                    let audio_data = data[9..].to_vec();
                    return Ok(Self::BroadcastAudio {
                        sender_id,
                        audio_data,
                    });
                }

                if data.len() < 9 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: msg_type,
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
            // 0x02: ClientTargetedAudio
            0x02 => {
                if data.len() < 34 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x02,
                        actual: data.len(),
                        expected: 34,
                    });
                }
                let target_bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let target_address = UserAddress::from_bytes(target_bytes);
                let codec_id = data[33];
                let audio_data = data[34..].to_vec();

                Ok(Self::ClientTargetedAudio {
                    target_address,
                    codec_id,
                    audio_data,
                })
            }
            // 0x03: ServerTargetedAudio (or legacy ClientTargetedAudio format if length < 74)
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

                if data.len() >= 74 {
                    let sender_id = u64::from_le_bytes(data[33..41].try_into().unwrap());
                    let sender_bytes: [u8; 32] = data[41..73].try_into().unwrap();
                    let sender_address = UserAddress::from_bytes(sender_bytes);
                    let codec_id = data[73];
                    let audio_data = data[74..].to_vec();

                    Ok(Self::ServerTargetedAudio {
                        target_address,
                        sender_id,
                        sender_address,
                        codec_id,
                        audio_data,
                    })
                } else {
                    // Fallback parse without codec_id byte
                    let audio_data = data[33..].to_vec();
                    Ok(Self::ClientTargetedAudio {
                        target_address,
                        codec_id: CODEC_OPUS,
                        audio_data,
                    })
                }
            }
            // 0x04: PeerTargetedAudio
            0x04 => {
                if data.len() >= 83 {
                    let sender_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
                    let origin_node = u64::from_le_bytes(data[9..17].try_into().unwrap());
                    let target_bytes: [u8; 32] = data[17..49].try_into().unwrap();
                    let target_address = UserAddress::from_bytes(target_bytes);
                    let sender_bytes: [u8; 32] = data[49..81].try_into().unwrap();
                    let sender_address = UserAddress::from_bytes(sender_bytes);
                    let codec_id = data[81];
                    let ttl = data[82];
                    let audio_data = data[83..].to_vec();

                    Ok(Self::PeerTargetedAudio {
                        sender_id,
                        origin_node,
                        target_address,
                        sender_address,
                        codec_id,
                        ttl,
                        audio_data,
                    })
                } else if data.len() >= 41 {
                    // Legacy CallRequest fallback if length < 83
                    let caller_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
                    let caller_bytes: [u8; 32] = data[9..41].try_into().unwrap();
                    let caller_address = UserAddress::from_bytes(caller_bytes);
                    Ok(Self::CallRequest {
                        caller_id,
                        caller_address,
                    })
                } else {
                    Err(ProtocolError::InsufficientLength {
                        packet_type: 0x04,
                        actual: data.len(),
                        expected: 41,
                    })
                }
            }
            // 0x05: CallRequest (or legacy CallAcceptResponse)
            0x05 => {
                if data.len() >= 41 {
                    let caller_id = u64::from_le_bytes(data[1..9].try_into().unwrap());
                    let caller_bytes: [u8; 32] = data[9..41].try_into().unwrap();
                    let caller_address = UserAddress::from_bytes(caller_bytes);
                    Ok(Self::CallRequest {
                        caller_id,
                        caller_address,
                    })
                } else if data.len() >= 33 {
                    let caller_bytes: [u8; 32] = data[1..33].try_into().unwrap();
                    let caller_address = UserAddress::from_bytes(caller_bytes);
                    Ok(Self::CallAcceptResponse { caller_address })
                } else {
                    Err(ProtocolError::InsufficientLength {
                        packet_type: 0x05,
                        actual: data.len(),
                        expected: 33,
                    })
                }
            }
            // 0x06: CallAcceptResponse
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
                Ok(Self::CallAcceptResponse { caller_address })
            }
            // 0x07: CallRejectResponse
            0x07 => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x07,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let caller_bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let caller_address = UserAddress::from_bytes(caller_bytes);
                Ok(Self::CallRejectResponse { caller_address })
            }
            // 0x08: CallAcceptedNotification
            0x08 => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x08,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let target_address = UserAddress::from_bytes(bytes);
                Ok(Self::CallAcceptedNotification { target_address })
            }
            // 0x09: CallRejectedNotification
            0x09 => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x09,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let target_address = UserAddress::from_bytes(bytes);
                Ok(Self::CallRejectedNotification { target_address })
            }
            // 0x0A: ConnectionError (or legacy 0xFF)
            0x0A | 0xFF => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: msg_type,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let target_address = UserAddress::from_bytes(bytes);
                Ok(Self::ConnectionError { target_address })
            }
            // 0x0B: CallHangup
            0x0B => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x0B,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let target_address = UserAddress::from_bytes(bytes);
                Ok(Self::CallHangup { target_address })
            }
            // 0x0C: CallEndedNotification
            0x0C => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x0C,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let target_address = UserAddress::from_bytes(bytes);
                Ok(Self::CallEndedNotification { target_address })
            }
            // 0x0D: RoomJoinRequest
            0x0D => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x0D,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let room_address = UserAddress::from_bytes(bytes);
                Ok(Self::RoomJoinRequest { room_address })
            }
            // 0x0E: RoomStateNotification
            0x0E => {
                if data.len() < 37 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x0E,
                        actual: data.len(),
                        expected: 37,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let room_address = UserAddress::from_bytes(bytes);
                let participant_count = u32::from_le_bytes(data[33..37].try_into().unwrap());
                Ok(Self::RoomStateNotification {
                    room_address,
                    participant_count,
                })
            }
            // 0x0F: RoomLeaveRequest
            0x0F => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x0F,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let room_address = UserAddress::from_bytes(bytes);
                Ok(Self::RoomLeaveRequest { room_address })
            }
            // 0x10: RoomGroupAudio
            0x10 => {
                if data.len() < 34 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x10,
                        actual: data.len(),
                        expected: 34,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let room_address = UserAddress::from_bytes(bytes);
                let codec_id = data[33];
                let audio_data = data[34..].to_vec();
                Ok(Self::RoomGroupAudio {
                    room_address,
                    codec_id,
                    audio_data,
                })
            }
            // 0x11: ActiveSpeakerNotice
            0x11 => {
                if data.len() < 33 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x11,
                        actual: data.len(),
                        expected: 33,
                    });
                }
                let bytes: [u8; 32] = data[1..33].try_into().unwrap();
                let room_address = UserAddress::from_bytes(bytes);

                let mut speaker_addresses = Vec::new();
                let mut offset = 33;
                while offset + 32 <= data.len() {
                    let s_bytes: [u8; 32] = data[offset..offset + 32].try_into().unwrap();
                    speaker_addresses.push(UserAddress::from_bytes(s_bytes));
                    offset += 32;
                }
                Ok(Self::ActiveSpeakerNotice {
                    room_address,
                    speaker_addresses,
                })
            }
            // 0x12: Ping
            0x12 => {
                if data.len() < 9 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x12,
                        actual: data.len(),
                        expected: 9,
                    });
                }
                let timestamp = u64::from_le_bytes(data[1..9].try_into().unwrap());
                Ok(Self::Ping { timestamp })
            }
            // 0x13: Pong
            0x13 => {
                if data.len() < 9 {
                    return Err(ProtocolError::InsufficientLength {
                        packet_type: 0x13,
                        actual: data.len(),
                        expected: 9,
                    });
                }
                let timestamp = u64::from_le_bytes(data[1..9].try_into().unwrap());
                Ok(Self::Pong { timestamp })
            }
            unknown => Err(ProtocolError::UnknownPacketType(unknown)),
        }
    }

    /// Encode `ProtocolPacket` into raw byte vector for DataChannel transmission according to WPIP-04.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::ClientAssignment {
                client_id,
                user_address,
            } => {
                let mut buf = Vec::with_capacity(41);
                buf.push(0x01);
                buf.extend_from_slice(&client_id.to_le_bytes());
                buf.extend_from_slice(&user_address.to_bytes());
                buf
            }
            Self::ClientTargetedAudio {
                target_address,
                codec_id,
                audio_data,
            } => {
                let mut buf = Vec::with_capacity(34 + audio_data.len());
                buf.push(0x02);
                buf.extend_from_slice(&target_address.to_bytes());
                buf.push(*codec_id);
                buf.extend_from_slice(audio_data);
                buf
            }
            Self::ServerTargetedAudio {
                target_address,
                sender_id,
                sender_address,
                codec_id,
                audio_data,
            } => {
                let mut buf = Vec::with_capacity(74 + audio_data.len());
                buf.push(0x03);
                buf.extend_from_slice(&target_address.to_bytes());
                buf.extend_from_slice(&sender_id.to_le_bytes());
                buf.extend_from_slice(&sender_address.to_bytes());
                buf.push(*codec_id);
                buf.extend_from_slice(audio_data);
                buf
            }
            Self::PeerTargetedAudio {
                sender_id,
                origin_node,
                target_address,
                sender_address,
                codec_id,
                ttl,
                audio_data,
            } => {
                let mut buf = Vec::with_capacity(83 + audio_data.len());
                buf.push(0x04);
                buf.extend_from_slice(&sender_id.to_le_bytes());
                buf.extend_from_slice(&origin_node.to_le_bytes());
                buf.extend_from_slice(&target_address.to_bytes());
                buf.extend_from_slice(&sender_address.to_bytes());
                buf.push(*codec_id);
                buf.push(*ttl);
                buf.extend_from_slice(audio_data);
                buf
            }
            Self::CallRequest {
                caller_id,
                caller_address,
            } => {
                let mut buf = Vec::with_capacity(41);
                buf.push(0x05);
                buf.extend_from_slice(&caller_id.to_le_bytes());
                buf.extend_from_slice(&caller_address.to_bytes());
                buf
            }
            Self::CallAcceptResponse { caller_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x06);
                buf.extend_from_slice(&caller_address.to_bytes());
                buf
            }
            Self::CallRejectResponse { caller_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x07);
                buf.extend_from_slice(&caller_address.to_bytes());
                buf
            }
            Self::CallAcceptedNotification { target_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x08);
                buf.extend_from_slice(&target_address.to_bytes());
                buf
            }
            Self::CallRejectedNotification { target_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x09);
                buf.extend_from_slice(&target_address.to_bytes());
                buf
            }
            Self::ConnectionError { target_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x0A);
                buf.extend_from_slice(&target_address.to_bytes());
                buf
            }
            Self::CallHangup { target_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x0B);
                buf.extend_from_slice(&target_address.to_bytes());
                buf
            }
            Self::CallEndedNotification { target_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x0C);
                buf.extend_from_slice(&target_address.to_bytes());
                buf
            }
            Self::RoomJoinRequest { room_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x0D);
                buf.extend_from_slice(&room_address.to_bytes());
                buf
            }
            Self::RoomStateNotification {
                room_address,
                participant_count,
            } => {
                let mut buf = Vec::with_capacity(37);
                buf.push(0x0E);
                buf.extend_from_slice(&room_address.to_bytes());
                buf.extend_from_slice(&participant_count.to_le_bytes());
                buf
            }
            Self::RoomLeaveRequest { room_address } => {
                let mut buf = Vec::with_capacity(33);
                buf.push(0x0F);
                buf.extend_from_slice(&room_address.to_bytes());
                buf
            }
            Self::RoomGroupAudio {
                room_address,
                codec_id,
                audio_data,
            } => {
                let mut buf = Vec::with_capacity(34 + audio_data.len());
                buf.push(0x10);
                buf.extend_from_slice(&room_address.to_bytes());
                buf.push(*codec_id);
                buf.extend_from_slice(audio_data);
                buf
            }
            Self::ActiveSpeakerNotice {
                room_address,
                speaker_addresses,
            } => {
                let mut buf = Vec::with_capacity(33 + 32 * speaker_addresses.len());
                buf.push(0x11);
                buf.extend_from_slice(&room_address.to_bytes());
                for addr in speaker_addresses {
                    buf.extend_from_slice(&addr.to_bytes());
                }
                buf
            }
            Self::Ping { timestamp } => {
                let mut buf = Vec::with_capacity(9);
                buf.push(0x12);
                buf.extend_from_slice(&timestamp.to_le_bytes());
                buf
            }
            Self::Pong { timestamp } => {
                let mut buf = Vec::with_capacity(9);
                buf.push(0x13);
                buf.extend_from_slice(&timestamp.to_le_bytes());
                buf
            }
            Self::BroadcastAudio {
                sender_id,
                audio_data,
            } => {
                let mut buf = Vec::with_capacity(9 + audio_data.len());
                buf.push(0x00);
                buf.extend_from_slice(&sender_id.to_le_bytes());
                buf.extend_from_slice(audio_data);
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
        assert_eq!(bytes[0], 0x01);

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
        assert_eq!(bytes[0], 0x00);

        let decoded = ProtocolPacket::decode(&bytes).expect("Failed to decode BroadcastAudio");
        assert_eq!(decoded, packet);
    }

    #[test]
    fn test_targeted_audio_roundtrips() {
        let target = UserAddress::generate_from_time();
        let sender = UserAddress::generate_from_time();

        let client_packet = ProtocolPacket::ClientTargetedAudio {
            target_address: target.clone(),
            codec_id: CODEC_OPUS,
            audio_data: vec![10, 20, 30],
        };
        let bytes = client_packet.encode();
        assert_eq!(bytes[0], 0x02);
        let decoded = ProtocolPacket::decode(&bytes).expect("Failed to decode ClientTargetedAudio");
        assert_eq!(decoded, client_packet);

        let server_packet = ProtocolPacket::ServerTargetedAudio {
            target_address: target.clone(),
            sender_id: 100,
            sender_address: sender.clone(),
            codec_id: CODEC_OPUS,
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

        let hangup = ProtocolPacket::CallHangup {
            target_address: addr.clone(),
        };
        assert_eq!(ProtocolPacket::decode(&hangup.encode()).unwrap(), hangup);

        let ended = ProtocolPacket::CallEndedNotification {
            target_address: addr.clone(),
        };
        assert_eq!(ProtocolPacket::decode(&ended.encode()).unwrap(), ended);
    }

    #[test]
    fn test_sfu_and_ping_pong_roundtrips() {
        let addr = UserAddress::generate_from_time();

        let join = ProtocolPacket::RoomJoinRequest {
            room_address: addr.clone(),
        };
        assert_eq!(ProtocolPacket::decode(&join.encode()).unwrap(), join);

        let ping = ProtocolPacket::Ping {
            timestamp: 987654321,
        };
        assert_eq!(ProtocolPacket::decode(&ping.encode()).unwrap(), ping);

        let pong = ProtocolPacket::Pong {
            timestamp: 987654321,
        };
        assert_eq!(ProtocolPacket::decode(&pong.encode()).unwrap(), pong);
    }

    #[test]
    fn test_decode_errors() {
        assert_eq!(ProtocolPacket::decode(&[]), Err(ProtocolError::EmptyPacket));
        assert_eq!(
            ProtocolPacket::decode(&[0x99]),
            Err(ProtocolError::UnknownPacketType(0x99))
        );
    }
}
