//! Client session module.
//!
//! Provides `ClientSession` encapsulating session-specific state (client ID, audio buffers)
//! to allow multiple independent client instances within a single process.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Holds state for an active client connection session.
#[derive(Debug, Clone)]
pub struct ClientSession {
    /// Audio ring buffer for receiving audio data from WebRTC DataChannel.
    pub audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    /// Assigned client ID received from server (u64::MAX if unassigned).
    pub client_id: Arc<AtomicU64>,
    /// Assigned temporary UserAddress received from server via ClientAssignment (0x01).
    pub user_address: Arc<Mutex<Option<crate::address::UserAddress>>>,
}

impl Default for ClientSession {
    fn default() -> Self {
        Self {
            audio_buffer: Arc::new(Mutex::new(VecDeque::new())),
            client_id: Arc::new(AtomicU64::new(u64::MAX)),
            user_address: Arc::new(Mutex::new(None)),
        }
    }
}

impl ClientSession {
    /// Create a new `ClientSession` with empty audio buffer and unassigned client ID.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset session state (clear buffer and reset client ID).
    pub fn reset(&self) {
        self.audio_buffer.lock().unwrap().clear();
        self.client_id.store(u64::MAX, Ordering::SeqCst);
        if let Ok(mut guard) = self.user_address.lock() {
            *guard = None;
        }
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
