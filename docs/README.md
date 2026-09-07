# WPIPs: web-phone Implementation Possibilities

**WPIPs** (web-phone Implementation Possibilities) are standard specification documents that define protocol formats, node behaviors, wire specifications, and standard implementations for the `web-phone` real-time WebRTC audio system.

Inspired by [NIPs (Nostr Implementation Possibilities)](https://github.com/nostr-protocol/nips).

---

## List of WPIPs

### Core Specifications
| WPIP | Title | Status | Summary |
| :--- | :--- | :---: | :--- |
| [WPIP-01](WPIP-01.md) | WPIP Architecture & Process | Standard | WPIP definitions, process, and system architecture overview |
| [WPIP-02](WPIP-02.md) | `wpdaemon` Core Specification | Standard | Minimal `wpdaemon` signaling, client registry, and basic audio routing |
| [WPIP-03](WPIP-03.md) | `wpclient` Core Specification | Standard | Minimal `wpclient` connection lifecycle and audio streaming |
| [WPIP-04](WPIP-04.md) | DataChannel Wire Protocol | Standard | Binary wire encoding and `ProtocolPacket` format specification |

### Extension & Feature Specifications
| WPIP | Title | Status | Summary |
| :--- | :--- | :---: | :--- |
| [WPIP-05](WPIP-05.md) | Inter-Daemon Peer Mesh | Optional | Multi-node daemon mesh interconnection and audio relaying |
| [WPIP-06](WPIP-06.md) | Embedded STUN/TURN Service | Optional | Built-in UDP STUN/TURN responder for NAT traversal in `wpdaemon` |
| [WPIP-07](WPIP-07.md) | Call Control & Capacity Constraints | Optional | Interactive call approval, rejection states, and 2-participant limit |

---

## Requirement Levels (RFC 2119)

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in all WPIP documents are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

---

## WPIP Statuses

- **Draft**: A proposal under active discussion and prototyping.
- **Standard**: An accepted specification defining standard `web-phone` behavior.
- **Optional**: An extension or feature proposal that implementations MAY adopt.
- **Experimental**: Experimental features or future capability proposals.
- **Deprecated**: Obsolete specifications.

---

## License

All WPIP specifications are published under the [MIT License](../LICENSE).
