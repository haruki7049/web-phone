# WPIP-02: wpdaemon Core Specification

`draft` `mandatory` `author:haruki7049`

---

## Abstract

This specification defines the minimal **Core Specification** for a compliant **`wpdaemon`** server node in the `web-phone` network. Advanced features (STUN/TURN, Call Control, Peer Mesh) are defined in separate extension WPIPs.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

---

## 1. Minimal Core Responsibilities

A compliant core `wpdaemon` implementation MUST satisfy only the following minimal requirements:

1. **HTTP/WebRTC Signaling (`POST /sdp`)**:
   - MUST accept WebRTC SDP Offers from clients and respond with a valid SDP Answer.
2. **Client Assignment (`ClientAssignment`)**:
   - MUST assign a unique 64-bit integer `client_id` and a 32-byte `UserAddress` to each connecting client.
   - MUST send `ProtocolPacket::ClientAssignment` (`0x01`) to the client immediately over DataChannel upon connection.
3. **Basic DataChannel Relay**:
   - MUST accept audio packets (`ClientTargetedAudio` - `0x02`) from clients and relay them to target clients (`ServerTargetedAudio` - `0x03`).
4. **Connection Cleanup**:
   - MUST unregister disconnected clients when their WebRTC PeerConnection terminates.

---

## 2. Minimal HTTP API

A core `wpdaemon` MUST expose the following HTTP endpoint:

### `POST /sdp` (Client Signaling)

- **Purpose**: Accepts a WebRTC SDP Offer from a `wpclient` and returns an SDP Answer.
- **Request**: `Content-Type: application/json`, Body: `RTCSessionDescription` (`type`, `sdp`).
- **Response**: `200 OK`, Body: `RTCSessionDescription` (`type`, `sdp`).

---

## 3. Optional Daemon Extensions

Implementations MAY support the following optional extensions defined in separate WPIPs:

- **STUN/TURN Service**: See [WPIP-07](WPIP-07.md) for embedded STUN/TURN server specifications.
- **Call Control & Capacity Constraints**: See [WPIP-08](WPIP-08.md) for 2-participant limits and call approval/rejection state management.
- **Inter-Daemon Peer Mesh**: See [WPIP-05](WPIP-05.md) for multi-daemon interconnection.
- **Address Directory Endpoint (`GET /addresses`)**: Implementations MAY expose `GET /addresses` returning a JSON list of registered client addresses.
