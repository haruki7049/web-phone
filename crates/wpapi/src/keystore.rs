//! WPIP-14: Client Key Store Encryption & Passphrase Key Derivation module.

use crate::address::UserKeypair;
use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Default path to `keystore.json` under OS configuration directory.
pub fn get_default_keystore_path() -> PathBuf {
    let proj_dirs = ProjectDirs::from("dev", "haruki7049", "wpclient")
        .expect("Failed to locate ProjectDirs for wpclient");
    let mut path = proj_dirs.config_dir().to_path_buf();
    path.push("keystore.json");
    path
}

/// Argon2id KDF parameters structure for JSON storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    pub salt: String,
    pub mem_limit_kib: u32,
    pub ops_limit: u32,
    pub parallelism: u32,
}

/// Crypto metadata structure for JSON storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CryptoMeta {
    pub kdf: String,
    pub kdf_params: KdfParams,
    pub cipher: String,
    pub nonce: String,
    pub ciphertext: String,
}

/// Encrypted Client Key Store JSON format (WPIP-14).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedKeyStore {
    pub version: u32,
    pub user_address: String,
    pub crypto: CryptoMeta,
}

/// Save an Ed25519 UserKeypair encrypted with a user passphrase to disk (WPIP-14).
pub fn save_encrypted_keystore(
    keypair: &UserKeypair,
    passphrase: &str,
    path: &Path,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let salt: [u8; 16] = rand::random();

    let params = Params::new(65536, 3, 4, Some(32))
        .map_err(|e| format!("Invalid Argon2id params: {}", e))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut derived_key = aes_gcm::Key::<Aes256Gcm>::default();
    argon2
        .hash_password_into(passphrase.as_bytes(), &salt, derived_key.as_mut_slice())
        .map_err(|e| format!("Argon2id key derivation failed: {}", e))?;

    let cipher = Aes256Gcm::new(&derived_key);
    let nonce_raw: [u8; 12] = rand::random();
    let nonce = aes_gcm::Nonce::from(nonce_raw);

    let raw_secret = keypair.to_bytes();
    let ciphertext = cipher
        .encrypt(&nonce, raw_secret.as_slice())
        .map_err(|e| format!("AES-256-GCM encryption failed: {}", e))?;

    let store = EncryptedKeyStore {
        version: 1,
        user_address: keypair.public_key_address().id,
        crypto: CryptoMeta {
            kdf: "argon2id".into(),
            kdf_params: KdfParams {
                salt: BASE64_STANDARD.encode(salt),
                mem_limit_kib: 65536,
                ops_limit: 3,
                parallelism: 4,
            },
            cipher: "aes-256-gcm".into(),
            nonce: BASE64_STANDARD.encode(nonce),
            ciphertext: BASE64_STANDARD.encode(ciphertext),
        },
    };

    let json_str = serde_json::to_string_pretty(&store)
        .map_err(|e| format!("Failed to serialize keystore JSON: {}", e))?;
    fs::write(path, json_str).map_err(|e| format!("Failed to write keystore file: {}", e))?;

    Ok(())
}

/// Load and decrypt an Ed25519 UserKeypair from an encrypted keystore file (WPIP-14).
pub fn load_encrypted_keystore(passphrase: &str, path: &Path) -> Result<UserKeypair, String> {
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read keystore file: {}", e))?;
    let store: EncryptedKeyStore = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse keystore JSON: {}", e))?;

    if store.version != 1 {
        return Err(format!("Unsupported keystore version: {}", store.version));
    }
    if store.crypto.kdf != "argon2id" {
        return Err(format!("Unsupported KDF algorithm: {}", store.crypto.kdf));
    }
    if store.crypto.cipher != "aes-256-gcm" {
        return Err(format!("Unsupported cipher: {}", store.crypto.cipher));
    }

    let salt = BASE64_STANDARD
        .decode(&store.crypto.kdf_params.salt)
        .map_err(|_| "Invalid Base64 in salt")?;
    let nonce_raw = BASE64_STANDARD
        .decode(&store.crypto.nonce)
        .map_err(|_| "Invalid Base64 in nonce")?;
    let ciphertext = BASE64_STANDARD
        .decode(&store.crypto.ciphertext)
        .map_err(|_| "Invalid Base64 in ciphertext")?;

    let nonce_bytes: [u8; 12] = nonce_raw
        .try_into()
        .map_err(|_| "Invalid nonce length (must be 12 bytes)")?;

    let params = Params::new(
        store.crypto.kdf_params.mem_limit_kib,
        store.crypto.kdf_params.ops_limit,
        store.crypto.kdf_params.parallelism,
        Some(32),
    )
    .map_err(|e| format!("Invalid Argon2id params: {}", e))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut derived_key = aes_gcm::Key::<Aes256Gcm>::default();
    argon2
        .hash_password_into(passphrase.as_bytes(), &salt, derived_key.as_mut_slice())
        .map_err(|e| format!("Argon2id key derivation failed: {}", e))?;

    let cipher = Aes256Gcm::new(&derived_key);

    let nonce = aes_gcm::Nonce::from(nonce_bytes);
    let decrypted_bytes = cipher
        .decrypt(&nonce, ciphertext.as_slice())
        .map_err(|_| "Decryption failed: Incorrect passphrase or corrupted keystore".to_string())?;

    let secret_array: [u8; 32] = decrypted_bytes
        .try_into()
        .map_err(|_| "Decrypted payload length is invalid for Ed25519 secret key")?;
    let keypair = UserKeypair::from_bytes(&secret_array);

    if keypair.public_key_address().id != store.user_address {
        return Err("Decrypted public key does not match keystore user_address".into());
    }

    Ok(keypair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load_encrypted_keystore_roundtrip() {
        let dir = tempdir().expect("Failed to create tempdir");
        let path = dir.path().join("keystore.json");
        let keypair = UserKeypair::generate();
        let passphrase = "my_secure_passphrase_123!";

        save_encrypted_keystore(&keypair, passphrase, &path)
            .expect("Failed to save encrypted keystore");
        assert!(path.exists());

        let loaded_keypair =
            load_encrypted_keystore(passphrase, &path).expect("Failed to load encrypted keystore");
        assert_eq!(
            loaded_keypair.public_key_address(),
            keypair.public_key_address()
        );
    }

    #[test]
    fn test_load_encrypted_keystore_wrong_passphrase() {
        let dir = tempdir().expect("Failed to create tempdir");
        let path = dir.path().join("keystore.json");
        let keypair = UserKeypair::generate();

        save_encrypted_keystore(&keypair, "correct_passphrase", &path).unwrap();

        let result = load_encrypted_keystore("wrong_passphrase", &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Decryption failed"));
    }

    #[test]
    fn test_load_encrypted_keystore_invalid_version_and_cipher() {
        let dir = tempdir().expect("Failed to create tempdir");
        let path = dir.path().join("invalid_ver.json");

        let mut store = EncryptedKeyStore {
            version: 99,
            user_address: "a1b2c3d4e5f6".into(),
            crypto: CryptoMeta {
                kdf: "argon2id".into(),
                kdf_params: KdfParams {
                    salt: "AAAA".into(),
                    mem_limit_kib: 65536,
                    ops_limit: 3,
                    parallelism: 4,
                },
                cipher: "aes-256-gcm".into(),
                nonce: "AAAA".into(),
                ciphertext: "AAAA".into(),
            },
        };

        fs::write(&path, serde_json::to_string(&store).unwrap()).unwrap();
        assert!(load_encrypted_keystore("pass", &path).is_err());

        store.version = 1;
        store.crypto.kdf = "pbkdf2".into();
        fs::write(&path, serde_json::to_string(&store).unwrap()).unwrap();
        assert!(load_encrypted_keystore("pass", &path).is_err());

        store.crypto.kdf = "argon2id".into();
        store.crypto.cipher = "chacha20-poly1305".into();
        fs::write(&path, serde_json::to_string(&store).unwrap()).unwrap();
        assert!(load_encrypted_keystore("pass", &path).is_err());
    }

    #[test]
    fn test_load_encrypted_keystore_corrupted_json_and_invalid_base64() {
        let dir = tempdir().expect("Failed to create tempdir");
        let path = dir.path().join("bad.json");

        // Non-existent file
        assert!(load_encrypted_keystore("pass", &dir.path().join("non_existent.json")).is_err());

        // Corrupted JSON
        fs::write(&path, "{ invalid json }").unwrap();
        assert!(load_encrypted_keystore("pass", &path).is_err());

        // Invalid Base64 in salt
        let bad_store = EncryptedKeyStore {
            version: 1,
            user_address: "a1b2c3d4e5f6".into(),
            crypto: CryptoMeta {
                kdf: "argon2id".into(),
                kdf_params: KdfParams {
                    salt: "!!!not_base64!!!".into(),
                    mem_limit_kib: 65536,
                    ops_limit: 3,
                    parallelism: 4,
                },
                cipher: "aes-256-gcm".into(),
                nonce: "AAAA".into(),
                ciphertext: "AAAA".into(),
            },
        };
        fs::write(&path, serde_json::to_string(&bad_store).unwrap()).unwrap();
        assert!(load_encrypted_keystore("pass", &path).is_err());
    }
}
