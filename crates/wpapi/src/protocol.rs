//! WebRTC DataChannel packet protocol module.
//!
//! Provides type-safe encoding and decoding for web-phone DataChannel network packets
//! strictly conforming to WPIP specifications (WPIP-01 through WPIP-21).

pub mod decoder;
pub mod encoder;
pub mod packet;

pub use packet::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::UserAddress;

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
        let speaker1 = UserAddress::generate_from_time();
        let speaker2 = UserAddress::generate_from_time();

        let join = ProtocolPacket::RoomJoinRequest {
            room_address: addr.clone(),
        };
        assert_eq!(ProtocolPacket::decode(&join.encode()).unwrap(), join);

        let room_state = ProtocolPacket::RoomStateNotification {
            room_address: addr.clone(),
            participant_count: 5,
        };
        assert_eq!(
            ProtocolPacket::decode(&room_state.encode()).unwrap(),
            room_state
        );

        let leave = ProtocolPacket::RoomLeaveRequest {
            room_address: addr.clone(),
        };
        assert_eq!(ProtocolPacket::decode(&leave.encode()).unwrap(), leave);

        let group_audio = ProtocolPacket::RoomGroupAudio {
            room_address: addr.clone(),
            codec_id: CODEC_OPUS,
            audio_data: vec![1, 2, 3, 4, 5],
        };
        assert_eq!(
            ProtocolPacket::decode(&group_audio.encode()).unwrap(),
            group_audio
        );

        let speaker_notice = ProtocolPacket::ActiveSpeakerNotice {
            room_address: addr.clone(),
            speaker_addresses: vec![speaker1, speaker2],
        };
        assert_eq!(
            ProtocolPacket::decode(&speaker_notice.encode()).unwrap(),
            speaker_notice
        );

        let video_frame = ProtocolPacket::VideoFrameData {
            target_address: addr.clone(),
            video_codec_id: 0x01,
            frame_data: vec![0xFF, 0xD8, 0xFF, 0xE0],
        };
        assert_eq!(
            ProtocolPacket::decode(&video_frame.encode()).unwrap(),
            video_frame
        );

        let peer_audio = ProtocolPacket::PeerTargetedAudio {
            sender_id: 10,
            origin_node: 20,
            target_address: addr.clone(),
            sender_address: addr.clone(),
            codec_id: CODEC_OPUS,
            ttl: 5,
            audio_data: vec![11, 22, 33],
        };
        assert_eq!(
            ProtocolPacket::decode(&peer_audio.encode()).unwrap(),
            peer_audio
        );

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

        let huge_packet = vec![0x02u8; MAX_PACKET_SIZE + 1];
        assert_eq!(
            ProtocolPacket::decode(&huge_packet),
            Err(ProtocolError::PacketTooLarge {
                actual: MAX_PACKET_SIZE + 1,
                max: MAX_PACKET_SIZE,
            })
        );

        let mut oversized_audio_packet = vec![0x02u8; 34];
        oversized_audio_packet.extend(vec![0xAAu8; MAX_AUDIO_PAYLOAD_SIZE + 1]);
        assert_eq!(
            ProtocolPacket::decode(&oversized_audio_packet),
            Err(ProtocolError::AudioPayloadTooLarge {
                actual: MAX_AUDIO_PAYLOAD_SIZE + 1,
                max: MAX_AUDIO_PAYLOAD_SIZE,
            })
        );
    }
}
