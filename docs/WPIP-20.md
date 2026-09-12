# WPIP-20: Screen Sharing & Video Track Protocol Extension

`experimental` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an experimental extension for `wpclient`, `wpapi`, and `wpdaemon` implementations to support **Screen Sharing and Video Track Streaming** over `ProtocolPacket` DataChannels or WebRTC media tracks.

This extension extends `web-phone` beyond audio-only communication, enabling high-efficiency screen capture sharing (VP8 / H.264 / AV1) alongside Opus audio streams during 1-to-1 calls and group rooms.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Video Packet Wire Format

Implementations supporting WPIP-20 MUST register new `ProtocolPacket` tag IDs for video frame payloads:

- **`0x14` (`VideoFrameData`)**: Encoded screen sharing / video frame payload (registered in [WPIP-04](WPIP-04.md) §3).

```text
+-------------------+-----------------------+--------------------+-------------------------+
| Tag (1b: 0x14)    | TargetAddress (32b)   | VideoCodecID (1b)  | FramePayload (bytes...) |
+-------------------+-----------------------+--------------------+-------------------------+
```

### Video Codec Identifiers

- `0x01`: VP8
- `0x02`: H.264
- `0x03`: AV1

______________________________________________________________________

## 2. Adaptive Quality & Bandwidth Management

- Clients SHOULD dynamically adjust frame rates and resolutions based on DataChannel throughput feedback.
- `wpdaemon` SFU implementations MAY selectively drop video frames for low-bandwidth participants while preserving high-priority audio forwarding.

______________________________________________________________________

## 3. Recommended Performance & Resource Control Guidelines

To prevent network bandwidth saturation and CPU exhaustion when streaming video tracks alongside real-time audio, implementations MUST adhere to the following priority guidelines:

1. **Frame Rate Capping for Screen Sharing**:

   - Clients SHOULD cap screen capture frame rates to **10–15 fps**. This preserves clear text legibility for code and documents while reducing bandwidth consumption by up to 50% compared to 30 fps.

1. **Concurrent Stream Limits per Room**:

   - `wpdaemon` SFU implementations RECOMMENDED enforcing a maximum limit on active concurrent video/screen sharing streams per room (RECOMMENDED default: **1 to 2 active streams per room**).
   - Limiting active video publishers prevents exponential bandwidth amplification across group room participants.

1. **Priority-Based Packet Forwarding & Low-Latency Audio Protection**:

   - `wpdaemon` SFU nodes MUST assign higher routing priority to audio packets (`0x02`, `0x10`, `0x15`) than video packets (`0x14`).
   - During network congestion, `wpdaemon` MUST selectively drop video frame payloads (`0x14`) to guarantee uninterrupted, low-latency audio transmission.
