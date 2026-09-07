# WPIP-06: Embedded STUN/TURN Service

`draft` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an optional extension for `wpdaemon` server nodes to provide an **Embedded STUN/TURN Service** for WebRTC NAT traversal.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. STUN Service Specification

A `wpdaemon` node implementing WPIP-06 MUST provide a UDP STUN responder service:

- **Protocol & Default Port**: UDP port `3478` (configurable).
- **RFC Compliance**: MUST comply with RFC 5389 STUN Binding Request (`0x0001`) processing.
- **XOR-MAPPED-ADDRESS**: MUST respond with STUN Binding Response (`0x0101`) containing `XOR-MAPPED-ADDRESS` (`0x0020`).
- **IPv4 / IPv6 Support**: MUST support XOR calculation for IPv4 (`0x01`) and IPv6 (`0x02`) address families based on incoming socket addresses.

______________________________________________________________________

## 2. Integration with `wpdaemon`

Implementations adopting WPIP-06 SHOULD allow enabling/disabling the STUN/TURN service via configuration (`turn_enabled = true / false`).
