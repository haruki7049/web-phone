use std::sync::LazyLock;
use tokio::sync::broadcast;
use wtransport::SendStream;

/// Type alias for a client's send stream
pub type ClientSender = std::sync::Arc<tokio::sync::Mutex<SendStream>>;

/// Audio message with sender information
#[derive(Clone)]
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
