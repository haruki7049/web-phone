# WPIP-17: Inter-Daemon Mutual TLS (mTLS) Mesh Authentication

`experimental` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an experimental extension for `wpdaemon` implementations to enforce **Mutual TLS (mTLS)** for inter-daemon peer mesh connections (WPIP-05).

By dynamically generating X.509 self-signed node certificates derived from each daemon's Ed25519 identity keypair and validating them against a `trusted_nodes` public key whitelist, `wpdaemon` nodes guarantee bidirectional transport-layer authentication without relying on external Public Key Infrastructure (PKI) or external Certificate Authorities (CAs).

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Node X.509 Certificate Generation

- Each `wpdaemon` instance MUST generate an ephemeral or persistent X.509 self-signed certificate derived directly from its Ed25519 node keypair (`node_id`).
- Certificates MUST include the node's 32-byte Ed25519 public key hex in the Certificate Subject Alternative Name (SAN) or Subject Key Identifier.
- Implementations MUST NOT require connection to external Certificate Authorities (CAs) or ACME/Let's Encrypt servers.

______________________________________________________________________

## 2. mTLS Handshake & Verification Procedure

When establishing inter-daemon connections (`POST /peer/sdp` or DataChannel):

1. **Client Certificate Request**: The receiving node (`Node B`) MUST configure its TLS listener to demand a valid client certificate during the TLS handshake.
1. **Mutual Verification**:
   - `Node A` MUST verify `Node B`'s server certificate against `Node B`'s known public key.
   - `Node B` MUST verify `Node A`'s client certificate against `Node A`'s public key.
1. **Whitelist Validation (`trusted_nodes`)**:
   - If a `trusted_nodes` whitelist is specified, both nodes MUST reject connection attempts if the peer node's public key is absent from the whitelist.
