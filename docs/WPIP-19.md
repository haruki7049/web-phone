# WPIP-19: Direct Client-to-Client ICE Transport Negotiation (Direct P2P Mode)

`experimental` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an experimental extension for `wpclient` and `wpdaemon` implementations to support **Direct Client-to-Client ICE Transport Negotiation (Direct P2P Mode)**.

While standard `web-phone` architectures relay audio payloads through `wpdaemon` server nodes (WPIP-02), Direct P2P Mode allows client endpoints in favorable NAT conditions to establish direct WebRTC PeerConnections between each other, bypassing daemon audio forwarding to achieve ultra-low latency.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Signaling Relay & Candidate Exchange

1. **Signaling via Daemon**: Clients MUST continue using `wpdaemon` for SDP offer/answer exchange and ICE Candidate signaling.
1. **Direct ICE Candidates**: Clients MAY include host and server-reflexive ICE candidates in their SDP offers to attempt direct UDP socket connection.
1. **Fallback to Daemon Relay**: If direct P2P ICE connectivity checks fail within `5000` milliseconds, clients MUST fallback to standard `wpdaemon` audio relay.
