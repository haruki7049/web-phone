//! WPIP-11: SFrame End-to-End Payload Encryption (E2EE) module for group audio.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use thiserror::Error;

/// SFrame encryption and decryption errors.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum SFrameError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: corrupted payload or invalid key")]
    DecryptionFailed,

    #[error("Invalid key size: expected 32 bytes")]
    InvalidKeySize,

    #[error("Invalid payload format")]
    InvalidPayloadFormat,
}

/// SFrame codec for AES-256-GCM E2EE payload encryption (WPIP-11).
pub struct SFrameCodec {
    cipher: Aes256Gcm,
}

impl SFrameCodec {
    /// Create a new `SFrameCodec` with a 32-byte room key.
    pub fn new(key: &[u8; 32]) -> Result<Self, SFrameError> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| SFrameError::InvalidKeySize)?;
        Ok(Self { cipher })
    }

    /// Encrypt plaintext audio payload using AES-256-GCM with a 12-byte nonce.
    pub fn encrypt(
        &self,
        nonce_bytes: &[u8; 12],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, SFrameError> {
        let nonce = Nonce::from(*nonce_bytes);
        self.cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| SFrameError::EncryptionFailed(e.to_string()))
    }

    /// Decrypt ciphertext audio payload using AES-256-GCM with a 12-byte nonce.
    pub fn decrypt(
        &self,
        nonce_bytes: &[u8; 12],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, SFrameError> {
        let nonce = Nonce::from(*nonce_bytes);
        self.cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| SFrameError::DecryptionFailed)
    }

    /// Package and encrypt a Zero-Knowledge SFU SFrame audio frame (WPIP-11).
    /// Structure: `[AudioEnergyByte (1b) | Encrypted SFrame Payload]`
    pub fn encrypt_frame(
        &self,
        nonce_bytes: &[u8; 12],
        energy_byte: u8,
        audio_pcm: &[u8],
    ) -> Result<Vec<u8>, SFrameError> {
        let ciphertext = self.encrypt(nonce_bytes, audio_pcm)?;
        let mut frame = Vec::with_capacity(1 + ciphertext.len());
        frame.push(energy_byte);
        frame.extend_from_slice(&ciphertext);
        Ok(frame)
    }

    /// Unpackage and decrypt a Zero-Knowledge SFU SFrame audio frame (WPIP-11).
    /// Returns `(AudioEnergyByte, DecryptedAudioPcm)`.
    pub fn decrypt_frame(
        &self,
        nonce_bytes: &[u8; 12],
        frame: &[u8],
    ) -> Result<(u8, Vec<u8>), SFrameError> {
        if frame.is_empty() {
            return Err(SFrameError::InvalidPayloadFormat);
        }
        let energy_byte = frame[0];
        let ciphertext = &frame[1..];
        let plaintext = self.decrypt(nonce_bytes, ciphertext)?;
        Ok((energy_byte, plaintext))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sframe_encryption_decryption_roundtrip() {
        let key = [0x42u8; 32];
        let nonce = [0x07u8; 12];
        let codec = SFrameCodec::new(&key).unwrap();

        let audio_pcm = vec![1.0f32.to_le_bytes(), 0.5f32.to_le_bytes()].concat();
        let energy = 180u8;

        let frame = codec.encrypt_frame(&nonce, energy, &audio_pcm).unwrap();
        assert_eq!(frame[0], energy);
        assert_ne!(&frame[1..], &audio_pcm[..]);

        let (dec_energy, dec_pcm) = codec.decrypt_frame(&nonce, &frame).unwrap();
        assert_eq!(dec_energy, energy);
        assert_eq!(dec_pcm, audio_pcm);
    }

    #[test]
    fn test_sframe_decryption_with_wrong_key_fails() {
        let key1 = [0x11u8; 32];
        let key2 = [0x22u8; 32];
        let nonce = [0x01u8; 12];

        let codec1 = SFrameCodec::new(&key1).unwrap();
        let codec2 = SFrameCodec::new(&key2).unwrap();

        let audio_pcm = vec![0xAB, 0xCD, 0xEF];
        let frame = codec1.encrypt_frame(&nonce, 100, &audio_pcm).unwrap();

        let res = codec2.decrypt_frame(&nonce, &frame);
        assert_eq!(res, Err(SFrameError::DecryptionFailed));
    }
}
