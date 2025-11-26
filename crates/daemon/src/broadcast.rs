use std::sync::LazyLock;
use tokio::sync::broadcast;
use wtransport::SendStream;

/// Type alias for a client's send stream
pub type ClientSender = std::sync::Arc<tokio::sync::Mutex<SendStream>>;

/// Broadcast channel for audio data
pub static AUDIO_BROADCAST: LazyLock<broadcast::Sender<Vec<u8>>> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(100);
    tx
});
