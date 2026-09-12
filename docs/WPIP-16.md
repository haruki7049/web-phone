# WPIP-16: Secp256k1 & Nostr Identity Authentication Support

`draft` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an optional extension for `wpdaemon`, `wpapi`, and `wpclient` implementations to support **secp256k1 Schnorr Signatures (BIP-340)** and **Nostr Key Identifiers (`npub` / `nsec`)** for WebRTC SDP signaling authentication.

While core WPIP specifications (WPIP-01, WPIP-02, WPIP-03) mandate Ed25519 key pairs for cryptographic identity, this proposal enables seamless interoperability with decentralized Nostr identities by accepting BIP-340 Schnorr-signed SDP offers over secp256k1 curves.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Cryptographic Identity Mapping

- **Nostr Public Key**: A 32-byte (64-character hex) X-only secp256k1 public key (the underlying raw bytes of a Bech32 `npub` string).
- **`UserAddress`**: Implementations MUST accept hex-encoded secp256k1 public keys as valid `UserAddress` identifiers. Because both Ed25519 public keys and X-only secp256k1 public keys pack into 32 raw bytes on the wire (`to_bytes()`), `UserAddress` handles both identity types uniformly across wire protocol packets.

______________________________________________________________________

## 2. HTTP Authorization Header Specification

Clients authenticating with a secp256k1 / Nostr key pair MUST send an `Authorization` HTTP header with the `WP-Secp256k1` scheme:

`Authorization: WP-Secp256k1 <NostrPubKeyHex>:<Timestamp>:<SchnorrSignatureHex>`

### 2.1 Parameters

- `NostrPubKeyHex`: 32-byte (64-character hex) X-only secp256k1 public key.
- `Timestamp`: UNIX epoch timestamp in seconds (decimal integer).
- `SchnorrSignatureHex`: 64-byte (128-character hex) BIP-340 Schnorr signature over the payload string `${Timestamp}:${sdp_offer}`.

### 2.2 Server Verification & Anti-Replay Procedure

When `wpdaemon` receives an SDP offer (`POST /sdp`) with a `WP-Secp256k1` authorization header:

1. **Header Parsing**: Parse `<NostrPubKeyHex>`, `<Timestamp>`, and `<SchnorrSignatureHex>`. If the header format is invalid, return `401 Unauthorized`.
1. **Time-Drift Check**: Calculate `|server_time - Timestamp|`. If the difference exceeds `300` seconds, reject with `401 Unauthorized` (Timestamp drift too large).
1. **Signature Verification**: Construct payload `${Timestamp}:${sdp_offer}` and verify `<SchnorrSignatureHex>` against `<NostrPubKeyHex>` using the secp256k1 BIP-340 Schnorr verification algorithm.
1. **Session Registration**: If valid, assign the client `UserAddress` matching `<NostrPubKeyHex>` and proceed with SDP handshake.

______________________________________________________________________

## 3. Backward Compatibility & Multi-Scheme Handshake

- `wpdaemon` MUST support both `WP-Ed25519` (WPIP-02 / WPIP-03) and `WP-Secp256k1` (WPIP-16) authorization schemes concurrently.
- Unauthenticated fallback connections MUST continue to generate temporary `UserAddress` instances as specified in WPIP-02.

______________________________________________________________________

## 4. Client `nsec` Key Ingestion & Keystore Storage

### 4.1 CLI & Environment Variable Key Ingestion

`wpclient` implementations MUST accept Nostr secret keys specified via:

- `--nostr-key <KEY>` (CLI flag, aliases `-n`, `--nsec`)
- `WPCLIENT_NOSTR_KEY` (Environment variable)

Implementations MUST accept both:

1. **Bech32 string**: NIP-19 `nsec1...` format decoded via BIP-173 Bech32 to 32 raw bytes.
1. **Hex string**: 64-character hexadecimal representation of the 32-byte secp256k1 secret key.

### 4.2 WPIP-14 Encrypted Keystore Integration

When persisting a Nostr secp256k1 identity key to `keystore.json` via WPIP-14:

- The JSON structure MUST include `"key_type": "secp256k1"`.
- The raw 32-byte secret key MUST be encrypted using Argon2id key derivation and AES-256-GCM authenticated encryption.

______________________________________________________________________

## 5. Security Considerations & Non-Transmission Proof

### 5.1 Non-Transmission of Secret Keys (`nsec`)

- **Strict Client-Side Signature Generation**: Secret keys (`nsec` or raw secp256k1 private keys) MUST NEVER be transmitted over the network or sent to `wpdaemon`.
- **Zero-Knowledge Signature Scheme**: Authentication relies exclusively on BIP-340 Schnorr signatures over the string payload `${Timestamp}:${sdp_offer}`.
- **Proof of Non-Transmission**:
  1. **Cryptographic Proof**: Under the Elliptic Curve Discrete Logarithm Problem (ECDLP), it is computationally intractable to derive the secret key $d$ from the signature $(R, s)$ and public key $P$.
  1. **Specification Proof**: The HTTP `Authorization` header specified in Section 2 (`WP-Secp256k1 <NostrPubKeyHex>:<Timestamp>:<SchnorrSignatureHex>`) contains only public identifiers and signatures.
  1. **Auditability**: Packet capture tools (such as Wireshark or `tcpdump`) and source code audits can independently confirm that raw 32-byte secret key bytes do not appear in HTTP headers or POST payloads.
