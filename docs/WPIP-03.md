# WPIP-03: wpclient Core Specification

`draft` `mandatory` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines the minimal **Core Specification** for a compliant **`wpclient`** application in the `web-phone` network, featuring mandatory Ed25519 cryptographic keypair management and signed HTTP signaling.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Minimal Core Requirements

A compliant core `wpclient` implementation MUST satisfy the following minimal requirements:

1. **Cryptographic Key Management**:
   - MUST generate or load an Ed25519 secret key and derive its corresponding 32-byte Ed25519 Public Key as its self-sovereign `UserAddress`.
   - MUST store the keypair securely (e.g., `~/.config/wpclient/keypair.toml`).
1. **Signed Signaling & Handshake**:
   - MUST sign the payload `"<Timestamp>:<SDP_OFFER_STRING>"` using its Ed25519 secret key.
   - MUST include the HTTP header `Authorization: WP-Ed25519 <PubKeyHex>:<Timestamp>:<SignatureHex>` when issuing `POST /sdp` Offers to `wpdaemon`.
1. **Registration**:
   - MUST receive and verify its `client_id` and assigned `UserAddress` from `ClientAssignment` (`0x01`).
1. **Audio Streaming**:
   - MUST capture audio input, format samples, and transmit `ClientTargetedAudio` (`0x02`) packets over DataChannel.
   - MUST receive `ServerTargetedAudio` (`0x03`) packets from DataChannel and play them to audio output.
1. **Graceful Disconnect**:
   - MUST close DataChannel and PeerConnection upon termination.

______________________________________________________________________

## 2. Minimal Audio Requirements

- **Sampling & Format**: SHOULD support `48,000 Hz` PCM 32-bit float (`f32`) audio.
- **Resampling**: If device sampling rates differ, `wpclient` MUST resample audio data before transmission or playback.

______________________________________________________________________

## 3. Optional Client Extensions

Implementations MAY support the following optional extensions defined in separate WPIPs:

- **Call Control & Interactive Prompting**: See [WPIP-07](WPIP-07.md) for handling `CallRequest`, user accept/reject prompts, and `auto_accept` settings.
- **Audio Device Discovery & Selection**: See client configuration specs for enumerating and selecting input/output audio hardware.
