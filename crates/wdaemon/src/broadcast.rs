//! Audio broadcast module.
//!
//! This module provides the broadcast channel infrastructure for
//! distributing audio data to all connected clients. It uses a
//! tokio broadcast channel for efficient fan-out.

use std::sync::LazyLock;
use tokio::sync::broadcast;
use wtransport::SendStream;

/// Type alias for a client's send stream.
///
/// This is wrapped in `Arc<Mutex>` to allow safe sharing between
/// the connection handler and broadcast tasks.
pub type ClientSender = std::sync::Arc<tokio::sync::Mutex<SendStream>>;

/// Audio message with sender information.
///
/// This struct encapsulates audio data along with the sender's ID,
/// allowing clients to identify and optionally filter their own audio.
#[derive(Clone, Debug)]
pub struct AudioMessage {
    /// ID of the client who sent this audio.
    ///
    /// This is used by clients to filter their own audio when
    /// echo back is disabled.
    pub sender_id: u64,
    /// Raw audio data bytes.
    ///
    /// The format is little-endian f32 samples.
    pub data: Vec<u8>,
}

/// Broadcast channel for audio data.
///
/// This channel distributes audio messages to all connected clients.
/// Each message includes the sender ID for echo filtering.
///
/// The channel capacity is 100 messages, which provides a buffer
/// for temporary network slowdowns while avoiding excessive memory use.
pub static AUDIO_BROADCAST: LazyLock<broadcast::Sender<AudioMessage>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(100);
    tx
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_message_creation() {
        let msg = AudioMessage {
            sender_id: 42,
            data: vec![1, 2, 3, 4],
        };
        assert_eq!(msg.sender_id, 42);
        assert_eq!(msg.data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_audio_message_clone() {
        let msg = AudioMessage {
            sender_id: 123,
            data: vec![10, 20, 30],
        };
        let cloned = msg.clone();
        assert_eq!(cloned.sender_id, msg.sender_id);
        assert_eq!(cloned.data, msg.data);
    }

    #[test]
    fn test_audio_message_empty_data() {
        let msg = AudioMessage {
            sender_id: 0,
            data: Vec::new(),
        };
        assert_eq!(msg.sender_id, 0);
        assert!(msg.data.is_empty());
    }

    #[tokio::test]
    async fn test_audio_broadcast_send_receive() {
        let tx = AUDIO_BROADCAST.clone();
        let mut rx = tx.subscribe();

        let msg = AudioMessage {
            sender_id: 1,
            data: vec![100, 200],
        };

        // Send message
        tx.send(msg.clone()).expect("Failed to send");

        // Receive message
        let received = rx.recv().await.expect("Failed to receive");
        assert_eq!(received.sender_id, 1);
        assert_eq!(received.data, vec![100, 200]);
    }
}
