//! Protocol packet definitions and constants.

use crate::address::UserAddress;
use thiserror::Error;

/// Default Opus Codec ID as defined in WPIP-04.
pub const CODEC_OPUS: u8 = 0x01;
/// PCM 32-bit float LE Codec ID.
pub const CODEC_PCM_F32LE: u8 = 0x00;
/// PCM 16-bit signed integer LE Codec ID.
pub const CODEC_PCM_S16LE: u8 = 0x02;

/// Maximum allowable total packet size (1 MB / 1,048,576 bytes) per WPIP-04.
pub const MAX_PACKET_SIZE: usize = 1048576;
/// Maximum allowable audio payload size (16 KiB).
pub const MAX_AUDIO_PAYLOAD_SIZE: usize = 16384;
/// Maximum allowable active speaker addresses per notice.
pub const MAX_SPEAKER_ADDRESSES: usize = 64;

/// Header byte offset for message type tag.
pub const OFFSET_MSG_TYPE: usize = 0;
/// Field size in bytes for `UserAddress` raw byte payload (32 bytes).
pub const LEN_USER_ADDR: usize = 32;
/// Field size in bytes for `client_id` (8 bytes LE u64).
pub const LEN_CLIENT_ID: usize = 8;
/// Field size in bytes for `participant_count` (4 bytes LE u32).
pub const LEN_PARTICIPANT_COUNT: usize = 4;
/// Field size in bytes for timestamp (8 bytes LE u64).
pub const LEN_TIMESTAMP: usize = 8;

/// Minimum packet length requirements for decoding validation.
pub const MIN_LEN_CLIENT_ASSIGNMENT: usize = 1 + LEN_CLIENT_ID; // 9
pub const FULL_LEN_CLIENT_ASSIGNMENT: usize = 1 + LEN_CLIENT_ID + LEN_USER_ADDR; // 41
pub const MIN_LEN_CLIENT_TARGETED_AUDIO: usize = 1 + LEN_USER_ADDR + 1; // 34
pub const MIN_LEN_SERVER_TARGETED_AUDIO: usize =
    1 + LEN_USER_ADDR + LEN_CLIENT_ID + LEN_USER_ADDR + 1; // 74
pub const MIN_LEN_PEER_TARGETED_AUDIO: usize =
    1 + LEN_CLIENT_ID + LEN_CLIENT_ID + LEN_USER_ADDR + LEN_USER_ADDR + 1 + 1; // 83
pub const MIN_LEN_CALL_REQUEST: usize = 1 + LEN_CLIENT_ID + LEN_USER_ADDR; // 41
pub const MIN_LEN_ADDRESS_ONLY_PACKET: usize = 1 + LEN_USER_ADDR; // 33
pub const MIN_LEN_ROOM_STATE_NOTIFICATION: usize = 1 + LEN_USER_ADDR + LEN_PARTICIPANT_COUNT; // 37
pub const MIN_LEN_PING_PONG: usize = 1 + LEN_TIMESTAMP; // 9
pub const MIN_LEN_VIDEO_FRAME_DATA: usize = 1 + LEN_USER_ADDR + 1; // 34
pub const MIN_LEN_ROOM_GROUP_AUDIO_E2EE: usize = 1 + LEN_USER_ADDR + 1 + 1; // 35

/// Errors that can occur during protocol packet decoding.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtocolError {
    /// Packet data is empty.
    #[error("Packet data is empty")]
    EmptyPacket,
    /// Packet size exceeds maximum allowable limit.
    #[error("Packet size {actual} exceeds maximum limit of {max} bytes")]
    PacketTooLarge { actual: usize, max: usize },
    /// Audio payload size exceeds maximum allowable limit.
    #[error("Audio payload size {actual} exceeds maximum limit of {max} bytes")]
    AudioPayloadTooLarge { actual: usize, max: usize },
    /// Active speaker count exceeds limit.
    #[error("Active speaker count {actual} exceeds limit of {max}")]
    TooManySpeakers { actual: usize, max: usize },
    /// Unknown or unsupported packet header byte.
    #[error("Unknown packet type: 0x{0:02x}")]
    UnknownPacketType(u8),
    /// Packet length is less than expected minimum.
    #[error(
        "Insufficient packet length for type 0x{packet_type:02x}: got {actual} bytes, expected at least {expected}"
    )]
    InsufficientLength {
        packet_type: u8,
        actual: usize,
        expected: usize,
    },
}

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
    /// 0x14: Video Frame Data (WPIP-20) [0x14, target_address (32b raw), video_codec_id (1b), frame_data...]
    VideoFrameData {
        target_address: UserAddress,
        video_codec_id: u8,
        frame_data: Vec<u8>,
    },
    /// 0x15: Room Group Audio E2EE (WPIP-11) [0x15, room_address (32b raw), codec_id (1b), audio_energy (1b), audio_data...]
    RoomGroupAudioE2EE {
        room_address: UserAddress,
        codec_id: u8,
        audio_energy: u8,
        audio_data: Vec<u8>,
    },
    /// Legacy Broadcast Audio [Broadcast tag, sender_id (8b LE), audio_data...]
    BroadcastAudio { sender_id: u64, audio_data: Vec<u8> },
}
