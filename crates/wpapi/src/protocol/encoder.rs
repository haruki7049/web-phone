//! Protocol packet encoder implementation using bytes::BufMut.

use super::packet::*;
use bytes::{BufMut, BytesMut};

impl ProtocolPacket {
    /// Encode `ProtocolPacket` into raw byte vector for DataChannel transmission according to WPIP-04.
    pub fn encode(&self) -> Vec<u8> {
        let buf = match self {
            Self::ClientAssignment {
                client_id,
                user_address,
            } => {
                let mut b = BytesMut::with_capacity(FULL_LEN_CLIENT_ASSIGNMENT);
                b.put_u8(0x01);
                b.put_u64_le(*client_id);
                b.put_slice(&user_address.to_bytes());
                b
            }
            Self::ClientTargetedAudio {
                target_address,
                codec_id,
                audio_data,
            } => {
                let mut b = BytesMut::with_capacity(1 + LEN_USER_ADDR + 1 + audio_data.len());
                b.put_u8(0x02);
                b.put_slice(&target_address.to_bytes());
                b.put_u8(*codec_id);
                b.put_slice(audio_data);
                b
            }
            Self::ServerTargetedAudio {
                target_address,
                sender_id,
                sender_address,
                codec_id,
                audio_data,
            } => {
                let mut b =
                    BytesMut::with_capacity(MIN_LEN_SERVER_TARGETED_AUDIO + audio_data.len());
                b.put_u8(0x03);
                b.put_slice(&target_address.to_bytes());
                b.put_u64_le(*sender_id);
                b.put_slice(&sender_address.to_bytes());
                b.put_u8(*codec_id);
                b.put_slice(audio_data);
                b
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
                let mut b = BytesMut::with_capacity(MIN_LEN_PEER_TARGETED_AUDIO + audio_data.len());
                b.put_u8(0x04);
                b.put_u64_le(*sender_id);
                b.put_u64_le(*origin_node);
                b.put_slice(&target_address.to_bytes());
                b.put_slice(&sender_address.to_bytes());
                b.put_u8(*codec_id);
                b.put_u8(*ttl);
                b.put_slice(audio_data);
                b
            }
            Self::CallRequest {
                caller_id,
                caller_address,
            } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_CALL_REQUEST);
                b.put_u8(0x05);
                b.put_u64_le(*caller_id);
                b.put_slice(&caller_address.to_bytes());
                b
            }
            Self::CallAcceptResponse { caller_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x06);
                b.put_slice(&caller_address.to_bytes());
                b
            }
            Self::CallRejectResponse { caller_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x07);
                b.put_slice(&caller_address.to_bytes());
                b
            }
            Self::CallAcceptedNotification { target_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x08);
                b.put_slice(&target_address.to_bytes());
                b
            }
            Self::CallRejectedNotification { target_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x09);
                b.put_slice(&target_address.to_bytes());
                b
            }
            Self::ConnectionError { target_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x0A);
                b.put_slice(&target_address.to_bytes());
                b
            }
            Self::CallHangup { target_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x0B);
                b.put_slice(&target_address.to_bytes());
                b
            }
            Self::CallEndedNotification { target_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x0C);
                b.put_slice(&target_address.to_bytes());
                b
            }
            Self::RoomJoinRequest { room_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x0D);
                b.put_slice(&room_address.to_bytes());
                b
            }
            Self::RoomStateNotification {
                room_address,
                participant_count,
            } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ROOM_STATE_NOTIFICATION);
                b.put_u8(0x0E);
                b.put_slice(&room_address.to_bytes());
                b.put_u32_le(*participant_count);
                b
            }
            Self::RoomLeaveRequest { room_address } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_ADDRESS_ONLY_PACKET);
                b.put_u8(0x0F);
                b.put_slice(&room_address.to_bytes());
                b
            }
            Self::RoomGroupAudio {
                room_address,
                codec_id,
                audio_data,
            } => {
                let mut b =
                    BytesMut::with_capacity(MIN_LEN_CLIENT_TARGETED_AUDIO + audio_data.len());
                b.put_u8(0x10);
                b.put_slice(&room_address.to_bytes());
                b.put_u8(*codec_id);
                b.put_slice(audio_data);
                b
            }
            Self::ActiveSpeakerNotice {
                room_address,
                speaker_addresses,
            } => {
                let mut b = BytesMut::with_capacity(
                    MIN_LEN_ADDRESS_ONLY_PACKET + LEN_USER_ADDR * speaker_addresses.len(),
                );
                b.put_u8(0x11);
                b.put_slice(&room_address.to_bytes());
                for addr in speaker_addresses {
                    b.put_slice(&addr.to_bytes());
                }
                b
            }
            Self::Ping { timestamp } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_PING_PONG);
                b.put_u8(0x12);
                b.put_u64_le(*timestamp);
                b
            }
            Self::Pong { timestamp } => {
                let mut b = BytesMut::with_capacity(MIN_LEN_PING_PONG);
                b.put_u8(0x13);
                b.put_u64_le(*timestamp);
                b
            }
            Self::VideoFrameData {
                target_address,
                video_codec_id,
                frame_data,
            } => {
                let mut b = BytesMut::with_capacity(1 + LEN_USER_ADDR + 1 + frame_data.len());
                b.put_u8(0x14);
                b.put_slice(&target_address.to_bytes());
                b.put_u8(*video_codec_id);
                b.put_slice(frame_data);
                b
            }
            Self::BroadcastAudio {
                sender_id,
                audio_data,
            } => {
                let mut b = BytesMut::with_capacity(1 + LEN_CLIENT_ID + audio_data.len());
                b.put_u8(0x00);
                b.put_u64_le(*sender_id);
                b.put_slice(audio_data);
                b
            }
        };
        buf.to_vec()
    }
}
