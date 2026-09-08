# WPIP-14: Client Key Store Encryption & Passphrase Key Derivation

`experimental` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an experimental security extension for `wpclient` and `wpapi` implementations to encrypt local Ed25519 client identity key pairs (`UserAddress` secret keys) on disk.

By deriving a 256-bit encryption key from a user-supplied **Passphrase** using **Argon2id** Key Derivation Function (KDF) and encrypting the secret key with **AES-256-GCM**, implementations prevent unauthorized key extraction if a client device or configuration file is physically or digitally compromised.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Key Derivation Function (Argon2id)

When encrypting or unlocking a client key store file:

- Implementations MUST derive a 32-byte (256-bit) encryption key from the user passphrase using **Argon2id**.
- Implementations MUST generate a unique 16-byte cryptographically secure random `salt` for each key store file.
- Recommended Argon2id parameters:
  - `Memory`: 64 MiB (65,536 KiB)
  - `Time Iterations`: 3
  - `Parallelism`: 4

______________________________________________________________________

## 2. Key Store File Format (JSON Schema)

Encrypted client key stores MUST be stored using the following JSON structure:

```json
{
  "version": 1,
  "user_address": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "crypto": {
    "kdf": "argon2id",
    "kdf_params": {
      "salt": "base64_encoded_16_byte_salt",
      "mem_limit_kib": 65536,
      "ops_limit": 3,
      "parallelism": 4
    },
    "cipher": "aes-256-gcm",
    "nonce": "base64_encoded_12_byte_nonce",
    "ciphertext": "base64_encoded_encrypted_ed25519_secret_key"
  }
}
```

______________________________________________________________________

## 3. Key Store Operations

- **Key Generation / Passphrase Lock**: `wpclient` prompts the user for a passphrase, derives the key via Argon2id, encrypts the raw 32-byte Ed25519 secret key with AES-256-GCM, and saves the encrypted JSON key store.
- **Key Unlocking**: Upon launching `wpclient call` or `wpclient room`, the client prompts the user for the passphrase to decrypt the key store into memory.
