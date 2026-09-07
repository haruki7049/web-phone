# WPIP-03: wpclient Core Specification

`draft` `mandatory` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines the minimal **Core Specification** for a compliant **`wpclient`** application in the `web-phone` network. Advanced features (Call Request prompts, auto-accept rules, device selection) are defined in separate extension WPIPs.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Minimal Core Requirements

A compliant core `wpclient` implementation MUST satisfy only the following minimal requirements:

1. **Signaling & Handshake**:
   - MUST send an SDP Offer to a `wpdaemon`'s `/sdp` endpoint and establish a WebRTC PeerConnection and DataChannel upon receiving the SDP Answer.
1. **Registration**:
   - MUST receive and store its `client_id` and assigned `UserAddress` from `ClientAssignment` (`0x01`).
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
- **Audio Device Discovery & Selection**: See [WPIP-06](WPIP-06.md) or client configuration specs for enumerating and selecting input/output audio hardware.
