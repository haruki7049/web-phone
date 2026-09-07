# WPIP-11: End-to-End Encryption (E2EE) for Group Rooms via SFrame

`experimental` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an experimental extension for `wpclient` implementations to support **End-to-End Encryption (E2EE)** in group calls (WPIP-08) using SFrame (Secure Frame) payload encryption.

E2EE guarantees that audio payloads are encrypted end-to-end between clients using a shared room key (`AES-256-GCM`), preventing `wpdaemon` SFU operators from eavesdropping on voice streams while preserving Zero-Knowledge SFU packet forwarding.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Zero-Knowledge SFU Payload Structure

When E2EE is enabled for a room:

- `wpclient` MUST encrypt Opus audio frames using **AES-256-GCM** before packaging them into `RoomGroupAudio` (`0x10`) packets.
- The 1-byte unencrypted packet header MUST contain an unencrypted 8-bit audio energy level indicator (`AudioEnergyByte`), allowing `wpdaemon` to perform Top-$K$ active speaker selection without decrypting the audio payload.

```text
+-------------------+-----------------------+--------------------+--------------------+----------------------------+
| Tag (1b: 0x10)    | RoomAddress (32b raw) | Codec ID (1b: u8)  | AudioEnergy (1b)   | Encrypted SFrame Payload   |
+-------------------+-----------------------+--------------------+--------------------+----------------------------+
```

______________________________________________________________________

## 2. Key Distribution

Room participants MUST exchange room encryption keys out-of-band or using MLS (Messaging Layer Security) protocol key exchanges signed with their Ed25519 `UserAddress` keys.
