# WPIP-18: Dynamic IP Blacklisting & Automated Abuse Mitigation

`experimental` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an experimental security extension for `wpdaemon` implementations to provide **Dynamic IP Blacklisting and Automated Abuse Mitigation (Fail2ban)**.

By tracking security violations—such as repeated Ed25519/secp256k1 signature verification failures, Token Bucket rate limit breaches (WPIP-15), or invalid STUN packet floods—`wpdaemon` nodes dynamically ban offending IP addresses to protect system resources and maintain network stability.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Violation Counter & Penalty Thresholds

Implementations MUST maintain an in-memory sliding window penalty table tracking IP address violation points:

- **Signature Failure Penalty**: `3` points per failed cryptographic signature verification attempt (`WP-Ed25519` or `WP-Secp256k1`).
- **Rate Limit Breach Penalty**: `1` point per HTTP / STUN rate limit denial (WPIP-15).
- **Ban Threshold**: When an IP accumulates `10` penalty points within a 60-second window, `wpdaemon` MUST add the IP to the active dynamic blacklist.

______________________________________________________________________

## 2. Dynamic Ban Execution & Drop Policy

- **Ban Duration**: Default dynamic ban duration MUST be `900` seconds (15 minutes).
- **Packet Drop Policy**: Incoming TCP connections and UDP packets originating from blacklisted IP addresses MUST be dropped at the lowest possible layer before HTTP parsing or STUN header processing.
- **Auto-Expiry**: Expired IP blacklist entries MUST be automatically pruned from memory.
