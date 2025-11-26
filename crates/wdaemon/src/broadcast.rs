use std::sync::LazyLock;
use tokio::sync::broadcast;
use wtransport::SendStream;

/// Type alias for a client's send stream
pub type ClientSender = std::sync::Arc<tokio::sync::Mutex<SendStream>>;

/// Audio message with sender information
#[derive(Clone, Debug)]
pub struct AudioMessage {
    /// ID of the client who sent this audio
    pub sender_id: u64,
    /// Audio data
    pub data: Vec<u8>,
}

/// Broadcast channel for audio data (includes sender ID for echo filtering)
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
