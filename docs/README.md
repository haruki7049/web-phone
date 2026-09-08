# WPIPs: web-phone Implementation Possibilities

**WPIPs** (web-phone Implementation Possibilities) are standard specification documents that define protocol formats, node behaviors, wire specifications, and standard implementations for the `web-phone` real-time WebRTC audio system.

______________________________________________________________________

## List of WPIPs

### Core Specifications

| WPIP | Title | Status | Summary |
| :--- | :--- | :---: | :--- |
| [WPIP-01](WPIP-01.md) | WPIP Architecture & Process | Standard | WPIP definitions, process, and system architecture overview |
| [WPIP-02](WPIP-02.md) | `wpdaemon` Core Specification | Standard | Minimal `wpdaemon` signaling, client registry, and basic audio routing |
| [WPIP-03](WPIP-03.md) | `wpclient` Core Specification | Standard | Minimal `wpclient` connection lifecycle and audio streaming |
| [WPIP-04](WPIP-04.md) | DataChannel Wire Protocol & Audio Codecs | Standard | Binary wire encoding, `ProtocolPacket` format, and Opus/PCM codec IDs |

### Extension & Feature Specifications

| WPIP | Title | Status | Summary |
| :--- | :--- | :---: | :--- |
| [WPIP-05](WPIP-05.md) | Inter-Daemon Peer Mesh | Optional | Multi-node daemon mesh interconnection, TTL hop limits, and audio relaying |
| [WPIP-06](WPIP-06.md) | Embedded STUN/TURN Service | Optional | Built-in UDP STUN/TURN responder for NAT traversal in `wpdaemon` |
| [WPIP-07](WPIP-07.md) | Call Control & Capacity Constraints | Optional | Interactive call approval, rejection states, and 2-participant limit |
| [WPIP-08](WPIP-08.md) | Group Call & SFU Extension | Optional | Selective Forwarding Unit (SFU) audio routing, Top-K active speaker selection, and room identity |

### Experimental Specifications

| WPIP | Title | Status | Summary |
| :--- | :--- | :---: | :--- |
| [WPIP-09](WPIP-09.md) | Connection Keep-Alive & Session Health Check | Experimental | Ping/Pong heartbeat packets over DataChannel for silent disconnection pruning |
| [WPIP-10](WPIP-10.md) | STUN/TURN Dynamic HMAC Token Authentication | Experimental | Time-limited HMAC-SHA1 credential allocation for embedded TURN relays |
| [WPIP-11](WPIP-11.md) | End-to-End Encryption (E2EE) for Group Rooms | Experimental | SFrame AES-256-GCM payload encryption for Zero-Knowledge SFU group calls |
| [WPIP-12](WPIP-12.md) | Dynamic Codec Negotiation & Capability Handshake | Experimental | Dynamic audio codec selection and capability negotiation during SDP signaling |
| [WPIP-13](WPIP-13.md) | Audio Device Hot-plugging & Dynamic Audio Route Switching | Experimental | Dynamic audio device event monitoring, fallback, and route switching |
| [WPIP-14](WPIP-14.md) | Client Key Store Encryption & Passphrase Key Derivation | Experimental | Argon2id & AES-256-GCM encrypted client identity key store format |

______________________________________________________________________

## Requirement Levels (RFC 2119)

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in all WPIP documents are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## WPIP Statuses

- **Draft**: A proposal under active discussion and prototyping.
- **Standard**: An accepted specification defining standard `web-phone` behavior.
- **Optional**: An extension or feature proposal that implementations MAY adopt.
- **Experimental**: Experimental features or future capability proposals.
- **Deprecated**: Obsolete specifications.

______________________________________________________________________

## License

All WPIP specifications are published under the [MIT License](../LICENSE).
