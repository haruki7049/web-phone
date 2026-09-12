//! Protocol packet decoder implementation using bytes::Buf.

use super::packet::*;
use crate::address::UserAddress;
use bytes::Buf;

impl ProtocolPacket {
    /// Safely decode raw DataChannel byte slice into a `ProtocolPacket`.
    pub fn decode(data: &[u8]) -> Result<Self, ProtocolError> {
        if data.is_empty() {
            return Err(ProtocolError::EmptyPacket);
        }
        if data.len() > MAX_PACKET_SIZE {
            return Err(ProtocolError::PacketTooLarge {
                actual: data.len(),
                max: MAX_PACKET_SIZE,
            });
        }

        let msg_type = data[OFFSET_MSG_TYPE];
        match msg_type {
            0x00 | 0x01 => decode_client_assignment(data, msg_type),
            0x02 => decode_client_targeted_audio(data),
            0x03 => decode_server_targeted_audio(data),
            0x04 => decode_peer_targeted_audio(data),
            0x05 => decode_call_request(data),
            0x06 => decode_call_accept_response(data),
            0x07 => decode_call_reject_response(data),
            0x08 => decode_call_accepted_notification(data),
            0x09 => decode_call_rejected_notification(data),
            0x0A | 0xFF => decode_connection_error(data, msg_type),
            0x0B => decode_call_hangup(data),
            0x0C => decode_call_ended_notification(data),
            0x0D => decode_room_join_request(data),
            0x0E => decode_room_state_notification(data),
            0x0F => decode_room_leave_request(data),
            0x10 => decode_room_group_audio(data),
            0x11 => decode_active_speaker_notice(data),
            0x12 => decode_ping(data),
            0x13 => decode_pong(data),
            0x14 => decode_video_frame_data(data),
            0x15 => decode_room_group_audio_e2ee(data),
            unknown => Err(ProtocolError::UnknownPacketType(unknown)),
        }
    }
}

fn parse_user_address(buf: &mut impl Buf) -> UserAddress {
    let mut bytes = [0u8; LEN_USER_ADDR];
    buf.copy_to_slice(&mut bytes);
    UserAddress::from_bytes(bytes)
}

fn decode_client_assignment(data: &[u8], msg_type: u8) -> Result<ProtocolPacket, ProtocolError> {
    if msg_type == 0x00 && data.len() >= 9 && data.len() < FULL_LEN_CLIENT_ASSIGNMENT {
        let mut buf = &data[1..];
        let sender_id = buf.get_u64_le();
        let audio_data = buf.copy_to_bytes(buf.remaining()).to_vec();
        return Ok(ProtocolPacket::BroadcastAudio {
            sender_id,
            audio_data,
        });
    }

    if data.len() < MIN_LEN_CLIENT_ASSIGNMENT {
        return Err(ProtocolError::InsufficientLength {
            packet_type: msg_type,
            actual: data.len(),
            expected: MIN_LEN_CLIENT_ASSIGNMENT,
        });
    }

    let mut buf = &data[1..];
    let client_id = buf.get_u64_le();

    if data.len() >= FULL_LEN_CLIENT_ASSIGNMENT {
        let user_address = parse_user_address(&mut buf);
        Ok(ProtocolPacket::ClientAssignment {
            client_id,
            user_address,
        })
    } else {
        let user_address = UserAddress::generate_from_time();
        Ok(ProtocolPacket::ClientAssignment {
            client_id,
            user_address,
        })
    }
}

fn decode_client_targeted_audio(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_CLIENT_TARGETED_AUDIO {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x02,
            actual: data.len(),
            expected: MIN_LEN_CLIENT_TARGETED_AUDIO,
        });
    }
    let mut buf = &data[1..];
    let target_address = parse_user_address(&mut buf);
    let codec_id = buf.get_u8();
    let audio_data = buf.copy_to_bytes(buf.remaining()).to_vec();
    if audio_data.len() > MAX_AUDIO_PAYLOAD_SIZE {
        return Err(ProtocolError::AudioPayloadTooLarge {
            actual: audio_data.len(),
            max: MAX_AUDIO_PAYLOAD_SIZE,
        });
    }
    Ok(ProtocolPacket::ClientTargetedAudio {
        target_address,
        codec_id,
        audio_data,
    })
}

fn decode_server_targeted_audio(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_CLIENT_TARGETED_AUDIO {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x03,
            actual: data.len(),
            expected: MIN_LEN_CLIENT_TARGETED_AUDIO,
        });
    }

    let mut buf = &data[1..];
    let target_address = parse_user_address(&mut buf);

    if data.len() >= MIN_LEN_SERVER_TARGETED_AUDIO {
        let sender_id = buf.get_u64_le();
        let sender_address = parse_user_address(&mut buf);
        let codec_id = buf.get_u8();
        let audio_data = buf.copy_to_bytes(buf.remaining()).to_vec();
        if audio_data.len() > MAX_AUDIO_PAYLOAD_SIZE {
            return Err(ProtocolError::AudioPayloadTooLarge {
                actual: audio_data.len(),
                max: MAX_AUDIO_PAYLOAD_SIZE,
            });
        }

        Ok(ProtocolPacket::ServerTargetedAudio {
            target_address,
            sender_id,
            sender_address,
            codec_id,
            audio_data,
        })
    } else {
        let audio_data = buf.copy_to_bytes(buf.remaining()).to_vec();
        if audio_data.len() > MAX_AUDIO_PAYLOAD_SIZE {
            return Err(ProtocolError::AudioPayloadTooLarge {
                actual: audio_data.len(),
                max: MAX_AUDIO_PAYLOAD_SIZE,
            });
        }
        Ok(ProtocolPacket::ClientTargetedAudio {
            target_address,
            codec_id: CODEC_OPUS,
            audio_data,
        })
    }
}

fn decode_peer_targeted_audio(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_PEER_TARGETED_AUDIO {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x04,
            actual: data.len(),
            expected: MIN_LEN_PEER_TARGETED_AUDIO,
        });
    }
    let mut buf = &data[1..];
    let sender_id = buf.get_u64_le();
    let origin_node = buf.get_u64_le();
    let target_address = parse_user_address(&mut buf);
    let sender_address = parse_user_address(&mut buf);
    let codec_id = buf.get_u8();
    let ttl = buf.get_u8();
    let audio_data = buf.copy_to_bytes(buf.remaining()).to_vec();
    if audio_data.len() > MAX_AUDIO_PAYLOAD_SIZE {
        return Err(ProtocolError::AudioPayloadTooLarge {
            actual: audio_data.len(),
            max: MAX_AUDIO_PAYLOAD_SIZE,
        });
    }

    Ok(ProtocolPacket::PeerTargetedAudio {
        sender_id,
        origin_node,
        target_address,
        sender_address,
        codec_id,
        ttl,
        audio_data,
    })
}

fn decode_call_request(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_CALL_REQUEST {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x05,
            actual: data.len(),
            expected: MIN_LEN_CALL_REQUEST,
        });
    }
    let mut buf = &data[1..];
    let caller_id = buf.get_u64_le();
    let caller_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::CallRequest {
        caller_id,
        caller_address,
    })
}

fn decode_call_accept_response(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x06,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let caller_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::CallAcceptResponse { caller_address })
}

fn decode_call_reject_response(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x07,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let caller_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::CallRejectResponse { caller_address })
}

fn decode_call_accepted_notification(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x08,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let target_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::CallAcceptedNotification { target_address })
}

fn decode_call_rejected_notification(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x09,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let target_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::CallRejectedNotification { target_address })
}

fn decode_connection_error(data: &[u8], msg_type: u8) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: msg_type,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let target_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::ConnectionError { target_address })
}

fn decode_call_hangup(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x0B,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let target_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::CallHangup { target_address })
}

fn decode_call_ended_notification(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x0C,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let target_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::CallEndedNotification { target_address })
}

fn decode_room_join_request(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x0D,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let room_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::RoomJoinRequest { room_address })
}

fn decode_room_state_notification(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ROOM_STATE_NOTIFICATION {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x0E,
            actual: data.len(),
            expected: MIN_LEN_ROOM_STATE_NOTIFICATION,
        });
    }
    let mut buf = &data[1..];
    let room_address = parse_user_address(&mut buf);
    let participant_count = buf.get_u32_le();
    Ok(ProtocolPacket::RoomStateNotification {
        room_address,
        participant_count,
    })
}

fn decode_room_leave_request(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x0F,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let room_address = parse_user_address(&mut buf);
    Ok(ProtocolPacket::RoomLeaveRequest { room_address })
}

fn decode_room_group_audio(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_CLIENT_TARGETED_AUDIO {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x10,
            actual: data.len(),
            expected: MIN_LEN_CLIENT_TARGETED_AUDIO,
        });
    }
    let mut buf = &data[1..];
    let room_address = parse_user_address(&mut buf);
    let codec_id = buf.get_u8();
    let audio_data = buf.copy_to_bytes(buf.remaining()).to_vec();
    if audio_data.len() > MAX_AUDIO_PAYLOAD_SIZE {
        return Err(ProtocolError::AudioPayloadTooLarge {
            actual: audio_data.len(),
            max: MAX_AUDIO_PAYLOAD_SIZE,
        });
    }
    Ok(ProtocolPacket::RoomGroupAudio {
        room_address,
        codec_id,
        audio_data,
    })
}

fn decode_active_speaker_notice(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ADDRESS_ONLY_PACKET {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x11,
            actual: data.len(),
            expected: MIN_LEN_ADDRESS_ONLY_PACKET,
        });
    }
    let mut buf = &data[1..];
    let room_address = parse_user_address(&mut buf);

    let remaining_bytes = buf.remaining();
    let speaker_count = remaining_bytes / LEN_USER_ADDR;
    if speaker_count > MAX_SPEAKER_ADDRESSES {
        return Err(ProtocolError::TooManySpeakers {
            actual: speaker_count,
            max: MAX_SPEAKER_ADDRESSES,
        });
    }

    let mut speaker_addresses = Vec::with_capacity(speaker_count);
    while buf.remaining() >= LEN_USER_ADDR {
        speaker_addresses.push(parse_user_address(&mut buf));
    }
    Ok(ProtocolPacket::ActiveSpeakerNotice {
        room_address,
        speaker_addresses,
    })
}

fn decode_ping(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_PING_PONG {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x12,
            actual: data.len(),
            expected: MIN_LEN_PING_PONG,
        });
    }
    let mut buf = &data[1..];
    let timestamp = buf.get_u64_le();
    Ok(ProtocolPacket::Ping { timestamp })
}

fn decode_pong(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_PING_PONG {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x13,
            actual: data.len(),
            expected: MIN_LEN_PING_PONG,
        });
    }
    let mut buf = &data[1..];
    let timestamp = buf.get_u64_le();
    Ok(ProtocolPacket::Pong { timestamp })
}

fn decode_video_frame_data(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_VIDEO_FRAME_DATA {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x14,
            actual: data.len(),
            expected: MIN_LEN_VIDEO_FRAME_DATA,
        });
    }
    let mut buf = &data[1..];
    let target_address = parse_user_address(&mut buf);
    let video_codec_id = buf.get_u8();
    let frame_data = buf.copy_to_bytes(buf.remaining()).to_vec();
    Ok(ProtocolPacket::VideoFrameData {
        target_address,
        video_codec_id,
        frame_data,
    })
}

fn decode_room_group_audio_e2ee(data: &[u8]) -> Result<ProtocolPacket, ProtocolError> {
    if data.len() < MIN_LEN_ROOM_GROUP_AUDIO_E2EE {
        return Err(ProtocolError::InsufficientLength {
            packet_type: 0x15,
            actual: data.len(),
            expected: MIN_LEN_ROOM_GROUP_AUDIO_E2EE,
        });
    }
    let mut buf = &data[1..];
    let room_address = parse_user_address(&mut buf);
    let codec_id = buf.get_u8();
    let audio_energy = buf.get_u8();
    let audio_data = buf.copy_to_bytes(buf.remaining()).to_vec();
    if audio_data.len() > MAX_AUDIO_PAYLOAD_SIZE {
        return Err(ProtocolError::AudioPayloadTooLarge {
            actual: audio_data.len(),
            max: MAX_AUDIO_PAYLOAD_SIZE,
        });
    }
    Ok(ProtocolPacket::RoomGroupAudioE2EE {
        room_address,
        codec_id,
        audio_energy,
        audio_data,
    })
}
