//! Audio broadcast module.
//!
//! This module provides the broadcast channel infrastructure for
//! distributing audio data to all connected clients and peer daemons.

use std::sync::LazyLock;
use tokio::sync::broadcast;
use wclient::UserAddress;

/// Audio message with sender, target address, and origin node information.
#[derive(Clone, Debug)]
pub struct AudioMessage {
    /// ID of the client who sent this audio.
    pub sender_id: u64,
    /// UserAddress of the client who sent this audio (if known).
    pub sender_address: Option<UserAddress>,
    /// Target UserAddress for targeted 1-to-1 audio call.
    pub target_address: UserAddress,
    /// ID of the daemon node where this audio originated.
    pub origin_node: u64,
    /// Raw audio data bytes (PCM f32 LE).
    pub data: Vec<u8>,
}

/// Broadcast channel for audio data across local clients and mesh nodes.
pub static AUDIO_BROADCAST: LazyLock<broadcast::Sender<AudioMessage>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(1000);
    tx
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_message_creation() {
        let msg = AudioMessage {
            sender_id: 42,
            sender_address: None,
            target_address: UserAddress::new("target_id_12345"),
            origin_node: 1,
            data: vec![1, 2, 3, 4],
        };
        assert_eq!(msg.sender_id, 42);
        assert_eq!(msg.target_address.id, "target_id_12345");
        assert_eq!(msg.origin_node, 1);
        assert_eq!(msg.data, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_audio_broadcast_send_receive() {
        let tx = AUDIO_BROADCAST.clone();
        let mut rx = tx.subscribe();

        let msg = AudioMessage {
            sender_id: 1,
            sender_address: None,
            target_address: UserAddress::new("target_id_12345"),
            origin_node: 10,
            data: vec![100, 200],
        };

        tx.send(msg.clone()).expect("Failed to send");
        let received = rx.recv().await.expect("Failed to receive");
        assert_eq!(received.sender_id, 1);
        assert_eq!(received.target_address.id, "target_id_12345");
        assert_eq!(received.origin_node, 10);
        assert_eq!(received.data, vec![100, 200]);
    }
}
