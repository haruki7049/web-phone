//! C API bindings and core client library for web-phone WebRTC audio system.

pub mod address;
pub mod audio;
pub mod call;
pub mod client;
pub mod config;
pub mod error;
pub mod ffi;
pub mod keystore;
pub mod protocol;
pub mod resample;
pub mod session;
pub mod sframe;
pub mod turn_auth;
pub mod webrtc_session;

pub use address::{
    AntiReplayCache, AuthError, GLOBAL_ANTI_REPLAY_CACHE, MAX_TIMESTAMP_DRIFT_SECS, UserAddress,
    UserKeypair, build_authorization_header, verify_any_authorization_header,
    verify_any_authorization_header_with_cache, verify_authorization_header,
    verify_authorization_header_with_cache, verify_secp256k1_authorization_header,
    verify_secp256k1_authorization_header_with_cache,
};
pub use audio::AudioEngine;
pub use client::DaemonApiClient;
pub use config::{CONFIGURATION, Configuration, DEFAULT_CONFIG_PATH};
pub use error::WpapiError;
pub use ffi::*;
pub use keystore::{
    EncryptedKeyStore, KeyStoreError, get_default_keystore_path, load_encrypted_keystore,
    save_encrypted_keystore,
};
pub use protocol::{ProtocolError, ProtocolPacket};
pub use session::ClientSession;
pub use sframe::{SFrameCodec, SFrameError};
pub use turn_auth::{
    TurnCredential, generate_ephemeral_turn_credential, verify_ephemeral_turn_credential,
};
