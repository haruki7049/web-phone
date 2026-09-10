//! Client session module.
//!
//! Provides `ClientSession` encapsulating session-specific state (client ID, audio buffers)
//! to allow multiple independent client instances within a single process.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

/// Call status notifications emitted by `webrtc_session`.
#[derive(Debug, Clone)]
pub enum CallNotification {
    Accepted(crate::address::UserAddress),
    Rejected(crate::address::UserAddress),
    Error(crate::address::UserAddress, String),
    Hangup(crate::address::UserAddress),
}

/// Type alias for call notification channel sender.
pub type CallNotificationSender = mpsc::Sender<CallNotification>;

/// Type alias for incoming call prompt channel sender.
pub type IncomingCallSender = mpsc::Sender<(
    crate::address::UserAddress,
    tokio::sync::oneshot::Sender<bool>,
)>;

use crate::address::{UserAddress, UserKeypair};
use crate::protocol::ProtocolPacket;
use webrtc::data_channel::DataChannel;

/// Holds state for an active client connection session.
#[derive(Clone)]
pub struct ClientSession {
    /// Audio ring buffer for receiving audio data from WebRTC DataChannel.
    pub audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    /// Assigned client ID received from server (u64::MAX if unassigned).
    pub client_id: Arc<AtomicU64>,
    /// Assigned temporary UserAddress received from server via ClientAssignment (0x01).
    pub user_address: Arc<Mutex<Option<UserAddress>>>,
    /// Cryptographic identity keypair for this session.
    pub keypair: Arc<UserKeypair>,
    /// Active 1-to-1 call target user address.
    pub active_target: Arc<Mutex<Option<UserAddress>>>,
    /// Active SFU group room address.
    pub active_room: Arc<Mutex<Option<UserAddress>>>,
    /// Active open WebRTC DataChannel.
    pub data_channel: Arc<Mutex<Option<Arc<dyn DataChannel>>>>,
    /// Optional incoming call request sender for custom UI prompting.
    pub incoming_call_tx: Arc<Mutex<Option<IncomingCallSender>>>,
    /// Optional call notification sender for session state updates.
    pub call_notification_tx: Arc<Mutex<Option<CallNotificationSender>>>,
    /// Atomic input audio energy level (0.0..=1.0 stored as f32::to_bits).
    pub input_level: Arc<AtomicU32>,
    /// Atomic output audio energy level (0.0..=1.0 stored as f32::to_bits).
    pub output_level: Arc<AtomicU32>,
    /// Mute toggle flag for audio input capture.
    pub is_muted: Arc<AtomicBool>,
}

impl std::fmt::Debug for ClientSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientSession")
            .field("client_id", &self.client_id)
            .field("user_address", &self.user_address)
            .field("active_target", &self.active_target)
            .field("active_room", &self.active_room)
            .field("data_channel_open", &self.get_data_channel().is_some())
            .finish()
    }
}

impl Default for ClientSession {
    fn default() -> Self {
        Self::with_keypair(UserKeypair::generate())
    }
}

impl ClientSession {
    /// Create a new `ClientSession` with empty audio buffer and unassigned client ID.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `ClientSession` using a specific `UserKeypair`.
    pub fn with_keypair(keypair: UserKeypair) -> Self {
        Self {
            audio_buffer: Arc::new(Mutex::new(VecDeque::new())),
            client_id: Arc::new(AtomicU64::new(u64::MAX)),
            user_address: Arc::new(Mutex::new(None)),
            keypair: Arc::new(keypair),
            active_target: Arc::new(Mutex::new(None)),
            active_room: Arc::new(Mutex::new(None)),
            data_channel: Arc::new(Mutex::new(None)),
            incoming_call_tx: Arc::new(Mutex::new(None)),
            call_notification_tx: Arc::new(Mutex::new(None)),
            input_level: Arc::new(AtomicU32::new(0)),
            output_level: Arc::new(AtomicU32::new(0)),
            is_muted: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Reset session state (clear buffer and reset client ID).
    pub fn reset(&self) {
        self.audio_buffer.lock().unwrap().clear();
        self.client_id.store(u64::MAX, Ordering::SeqCst);
        if let Ok(mut guard) = self.user_address.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.active_target.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.active_room.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.data_channel.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.incoming_call_tx.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.call_notification_tx.lock() {
            *guard = None;
        }
        self.input_level.store(0, Ordering::Relaxed);
        self.output_level.store(0, Ordering::Relaxed);
        self.is_muted.store(false, Ordering::Relaxed);
    }

    /// Set current input audio energy level.
    pub fn set_input_level(&self, level: f32) {
        self.input_level.store(level.to_bits(), Ordering::Relaxed);
    }

    /// Get current input audio energy level.
    pub fn get_input_level(&self) -> f32 {
        f32::from_bits(self.input_level.load(Ordering::Relaxed))
    }

    /// Set current output audio energy level.
    pub fn set_output_level(&self, level: f32) {
        self.output_level.store(level.to_bits(), Ordering::Relaxed);
    }

    /// Get current output audio energy level.
    pub fn get_output_level(&self) -> f32 {
        f32::from_bits(self.output_level.load(Ordering::Relaxed))
    }

    /// Set microphone mute state.
    pub fn set_muted(&self, muted: bool) {
        self.is_muted.store(muted, Ordering::Relaxed);
    }

    /// Check if microphone is muted.
    pub fn is_muted(&self) -> bool {
        self.is_muted.load(Ordering::Relaxed)
    }

    /// Set active 1-to-1 call target address.
    pub fn set_target_address(&self, target: Option<UserAddress>) {
        if let Ok(mut guard) = self.active_target.lock() {
            *guard = target;
        }
    }

    /// Get active 1-to-1 call target address if set.
    pub fn get_target_address(&self) -> Option<UserAddress> {
        self.active_target.lock().ok()?.clone()
    }

    /// Set active SFU group room address.
    pub fn set_room_address(&self, room: Option<UserAddress>) {
        if let Ok(mut guard) = self.active_room.lock() {
            *guard = room;
        }
    }

    /// Get active SFU group room address if set.
    pub fn get_room_address(&self) -> Option<UserAddress> {
        self.active_room.lock().ok()?.clone()
    }

    /// Set active WebRTC `DataChannel`.
    pub fn set_data_channel(&self, dc: Arc<dyn DataChannel>) {
        if let Ok(mut guard) = self.data_channel.lock() {
            *guard = Some(dc);
        }
    }

    /// Get active WebRTC `DataChannel` if set.
    pub fn get_data_channel(&self) -> Option<Arc<dyn DataChannel>> {
        self.data_channel.lock().ok()?.clone()
    }

    /// Send a `ProtocolPacket` over the active WebRTC DataChannel.
    pub async fn send_packet(&self, packet: &ProtocolPacket) -> anyhow::Result<()> {
        let dc = self
            .get_data_channel()
            .ok_or_else(|| anyhow::anyhow!("DataChannel is not open"))?;
        dc.send(bytes::BytesMut::from(packet.encode().as_slice()))
            .await?;
        Ok(())
    }

    /// Set an incoming call handler sender.
    pub fn set_incoming_call_handler(&self, tx: IncomingCallSender) {
        if let Ok(mut guard) = self.incoming_call_tx.lock() {
            *guard = Some(tx);
        }
    }

    /// Get current incoming call handler sender if set.
    pub fn get_incoming_call_handler(&self) -> Option<IncomingCallSender> {
        self.incoming_call_tx.lock().ok()?.clone()
    }

    /// Set a call notification handler sender.
    pub fn set_call_notification_handler(&self, tx: CallNotificationSender) {
        if let Ok(mut guard) = self.call_notification_tx.lock() {
            *guard = Some(tx);
        }
    }

    /// Get current call notification handler sender if set.
    pub fn get_call_notification_handler(&self) -> Option<CallNotificationSender> {
        self.call_notification_tx.lock().ok()?.clone()
    }

    /// Get current client ID if assigned by server.
    pub fn get_client_id(&self) -> Option<u64> {
        let id = self.client_id.load(Ordering::SeqCst);
        if id == u64::MAX { None } else { Some(id) }
    }

    /// Get current assigned temporary UserAddress if assigned by server.
    pub fn get_user_address(&self) -> Option<crate::address::UserAddress> {
        self.user_address.lock().ok()?.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_session_lifecycle() {
        let session = ClientSession::new();
        assert_eq!(session.get_client_id(), None);

        session.client_id.store(12345, Ordering::SeqCst);
        assert_eq!(session.get_client_id(), Some(12345));

        session.audio_buffer.lock().unwrap().push_back(0.5);
        assert_eq!(session.audio_buffer.lock().unwrap().len(), 1);

        session.reset();
        assert_eq!(session.get_client_id(), None);
        assert_eq!(session.audio_buffer.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_multiple_independent_sessions() {
        let session1 = ClientSession::new();
        let session2 = ClientSession::new();

        session1.client_id.store(100, Ordering::SeqCst);
        session2.client_id.store(200, Ordering::SeqCst);

        assert_eq!(session1.get_client_id(), Some(100));
        assert_eq!(session2.get_client_id(), Some(200));

        session1.audio_buffer.lock().unwrap().push_back(1.0);
        session2.audio_buffer.lock().unwrap().push_back(2.0);
        session2.audio_buffer.lock().unwrap().push_back(3.0);

        assert_eq!(session1.audio_buffer.lock().unwrap().len(), 1);
        assert_eq!(session2.audio_buffer.lock().unwrap().len(), 2);

        session1.reset();
        assert_eq!(session1.get_client_id(), None);
        assert_eq!(session1.audio_buffer.lock().unwrap().len(), 0);

        // session2 remains unchanged
        assert_eq!(session2.get_client_id(), Some(200));
        assert_eq!(session2.audio_buffer.lock().unwrap().len(), 2);
    }
}
