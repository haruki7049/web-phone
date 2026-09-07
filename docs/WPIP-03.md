# WPIP-03: wpclient Core Specification

`draft` `mandatory` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines the minimal **Core Specification** for a compliant **`wpclient`** application in the `web-phone` network, featuring mandatory Ed25519 cryptographic keypair management, signed HTTPS signaling, and TLS security.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Minimal Core Requirements

A compliant core `wpclient` implementation MUST satisfy the following minimal requirements:

1. **Cryptographic Key Management**:
   - MUST generate or load an Ed25519 secret key and derive its corresponding 32-byte Ed25519 Public Key as its self-sovereign `UserAddress`.
   - MUST store the keypair securely (e.g., `~/.config/wpclient/keypair.toml`).
1. **Signed HTTPS Signaling & Handshake**:
   - MUST issue `POST /sdp` requests over **HTTPS (TLS 1.2 / TLS 1.3)** for all non-loopback server addresses.
   - MUST sign the payload `"<Timestamp>:<SDP_OFFER_STRING>"` using its Ed25519 secret key.
   - MUST include the HTTP header `Authorization: WP-Ed25519 <PubKeyHex>:<Timestamp>:<SignatureHex>` when issuing `POST /sdp` Offers to `wpdaemon`.
1. **TLS Certificate Verification**:
   - MUST verify server TLS certificates by default.
   - MAY support an `--insecure-tls` flag or custom CA pinning option to connect to servers using self-signed TLS certificates.
1. **Registration**:
   - MUST receive and verify its `client_id` and assigned `UserAddress` from `ClientAssignment` (`0x01`).
1. **Audio Streaming**:
   - MUST capture audio input, format samples, and transmit `ClientTargetedAudio` (`0x02`) packets over DataChannel.
   - MUST receive `ServerTargetedAudio` (`0x03`) packets from DataChannel and play them to audio output.
1. **Graceful Disconnect**:
   - MUST close DataChannel and PeerConnection upon termination.

______________________________________________________________________

## 2. Minimal Audio & DSP Processing Requirements

- **Sampling & Frame Size**: `wpclient` SHOULD target **48,000 Hz** audio sampling rate with a standard **20 ms frame size** (960 samples per channel at 48 kHz).
- **Audio Codec**: MUST include the 1-byte `Codec ID` in audio packets. `wpclient` SHOULD support **`OPUS` (`0x01`)** encoding and decoding to minimize bandwidth consumption. It MAY also support `PCM_F32LE` (`0x00`) for uncompressed low-latency streaming.
- **Resampling**: If the physical audio device sampling rate differs from 48,000 Hz (e.g., 44,100 Hz, 16,000 Hz), `wpclient` MUST resample audio buffers to 48,000 Hz prior to Opus encoding or transmission.
- **Acoustic Echo Cancellation (AEC)**: When operating in hands-free mode (loudspeaker output and microphone capture enabled without headphones), `wpclient` SHOULD enable software Acoustic Echo Cancellation (AEC) (e.g., via OS-native audio APIs or `webrtc-audio-processing`) to eliminate acoustic feedback.
- **Noise Suppression & Gain Control (NS/AGC)**: `wpclient` MAY enable Noise Suppression (NS) and Automatic Gain Control (AGC) to stabilize input volume levels and suppress background stationary noise.

______________________________________________________________________

## 3. Short ID Resolution & User Interface Expectations

- **Short ID Input**: `wpclient` MAY allow users to specify target addresses using a Short ID prefix (minimum **12 hexadecimal characters**).
- **Pre-flight Resolution**: `wpclient` SHOULD resolve Short IDs to full 32-byte raw `UserAddress` values (via `GET /addresses` or local address cache) before populating DataChannel wire packets.
- **Ambiguity Handling**: Upon receiving an ambiguity error (`409 Conflict` or `AddressAmbiguousError`) from `wpdaemon`, `wpclient` MUST prompt the user to input additional hexadecimal characters or the full 64-character address.

______________________________________________________________________

## 4. Optional Client Extensions

Implementations MAY support the following optional extensions defined in separate WPIPs:

- **Call Control & Interactive Prompting**: See [WPIP-07](WPIP-07.md) for handling `CallRequest`, user accept/reject prompts, and `auto_accept` settings.
- **Group Call & SFU Interaction**: See [WPIP-08](WPIP-08.md) for joining group rooms (`RoomJoinRequest`) and handling active speaker notices.
- **Audio Device Discovery & Selection**: See client configuration specs for enumerating and selecting input/output audio hardware.
