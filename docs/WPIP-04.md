# WPIP-04: DataChannel Wire Protocol & Audio Codecs

`draft` `mandatory` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines the binary wire protocol, packet encoding, and audio codec identification specifications for **`ProtocolPacket`** transmitted over WebRTC DataChannels in the `web-phone` network.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Encoding Conventions & Cryptographic Identity

- **Endianness**: All multi-byte integer fields (`client_id`, `origin_node`, etc.) MUST be encoded in **Little Endian**.
- **Header Tag**: The first byte (`Index 0`) of every packet MUST contain an 8-bit unsigned integer (`u8`) tag identifying the packet type.
- **`UserAddress` Ed25519 Raw Binary Representation**: When packed into DataChannel wire packets, `UserAddress` MUST be represented as **32 raw bytes** (fixed-length binary byte array `[u8; 32]`) corresponding to the client's Ed25519 Public Key.

______________________________________________________________________

## 2. Audio Codec Identifiers (Codec ID)

Audio packets (`ClientTargetedAudio` - `0x02`, `ServerTargetedAudio` - `0x03`, `PeerTargetedAudio` - `0x04`) MUST contain a 1-byte `Codec ID` field preceding the audio payload.

| Codec ID (Hex) | Codec Name | Bitrate Range | Description |
| :---: | :--- | :---: | :--- |
| `0x00` | `PCM_F32LE` | ~1.536 Mbps | Raw 48kHz 32-bit float LE uncompressed PCM audio |
| `0x01` | `OPUS` | 16 – 64 kbps | **RECOMMENDED Standard**. Opus compressed frame for VoIP |
| `0x02` | `PCM_S16LE` | ~768 kbps | Raw 16-bit signed integer LE uncompressed PCM audio |

______________________________________________________________________

## 3. Packet Types and Tag Mapping

| Tag (Hex) | Packet Name | Sender | Receiver | Total Fixed Header Length | Description |
| :---: | :--- | :---: | :---: | :---: | :--- |
| `0x01` | `ClientAssignment` | Daemon | Client | 41 Bytes | Assigns `client_id` and generated `UserAddress` |
| `0x02` | `ClientTargetedAudio` | Client | Daemon | 34 + N Bytes | Audio sent from client to target (Includes Codec ID) |
| `0x03` | `ServerTargetedAudio` | Daemon | Client | 74 + N Bytes | Routed audio payload delivered to client (Includes Codec ID) |
| `0x04` | `PeerTargetedAudio` | Daemon | Daemon | 83 + N Bytes | Audio relay packet between mesh nodes (Includes Codec ID & TTL) |
| `0x05` | `CallRequest` | Daemon | Client | 41 Bytes | Notification of an incoming call request |
| `0x06` | `CallAcceptResponse` | Client | Daemon | 33 Bytes | User acceptance response for a call request |
| `0x07` | `CallRejectResponse` | Client | Daemon | 33 Bytes | User rejection response for a call request |
| `0x08` | `CallAcceptedNotification` | Daemon | Client | 33 Bytes | Notification to caller that target accepted call |
| `0x09` | `CallRejectedNotification` | Daemon | Client | 33 Bytes | Notification to caller that target rejected call |
| `0x0A` | `ConnectionError` | Daemon | Client | 33 Bytes | Connection rejection (e.g., maximum capacity reached) |
| `0x0B` | `CallHangup` | Client | Daemon | 33 Bytes | Client signals termination of current call session |
| `0x0C` | `CallEndedNotification` | Daemon | Client | 33 Bytes | Notification to peer client that call session ended |
| `0x0D` | `RoomJoinRequest` | Client | Daemon | 33 Bytes | Request to join a group room (See WPIP-08) |
| `0x0E` | `RoomStateNotification` | Daemon | Client | 37 Bytes | Room state update notification (See WPIP-08) |
| `0x0F` | `RoomLeaveRequest` | Client | Daemon | 33 Bytes | Request to leave a group room (See WPIP-08) |
| `0x10` | `RoomGroupAudio` | Client | Daemon / Client | 34 + N Bytes | Group audio packet tagged with `RoomAddress` (See WPIP-08) |
| `0x11` | `ActiveSpeakerNotice` | Daemon | Client | 33 + 32×K Bytes | Active speaker addresses notification (See WPIP-08) |

______________________________________________________________________

## 4. Detailed Byte Layouts

### 4.1 `ClientAssignment` (`0x01`)

```text
+-------------------+--------------------+-----------------------+
| Tag (1b: 0x01)    | Client ID (8b: u64)| UserAddress (32b raw) |
+-------------------+--------------------+-----------------------+
```

### 4.2 `ClientTargetedAudio` (`0x02`)

```text
+-------------------+-----------------------+--------------------+--------------------+
| Tag (1b: 0x02)    | Target Address (32b)  | Codec ID (1b: u8)  | Audio Payload (...) |
+-------------------+-----------------------+--------------------+--------------------+
```

### 4.3 `ServerTargetedAudio` (`0x03`)

```text
+-------------------+-----------------------+--------------------+-----------------------+--------------------+--------------------+
| Tag (1b: 0x03)    | Target Address (32b)  | Sender ID (8b: u64)| Sender Address (32b)  | Codec ID (1b: u8)  | Audio Payload (...) |
+-------------------+-----------------------+--------------------+-----------------------+--------------------+--------------------+
```

### 4.4 `PeerTargetedAudio` (`0x04`)

```text
+-------------------+--------------------+--------------------+-----------------------+-----------------------+--------------------+------------------+--------------------+
| Tag (1b: 0x04)    | Sender ID (8b: u64)| Origin Node (8b)   | Target Address (32b)  | Sender Address (32b)  | Codec ID (1b: u8)  | TTL (1b: u8)     | Audio Payload (...) |
+-------------------+--------------------+--------------------+-----------------------+-----------------------+--------------------+------------------+--------------------+
```

### 4.5 `CallRequest` (`0x05`)

```text
+-------------------+--------------------+-----------------------+
| Tag (1b: 0x05)    | Caller ID (8b: u64)| Caller Address (32b)  |
+-------------------+--------------------+-----------------------+
```

### 4.6 `CallAcceptResponse` (`0x06`) / `CallRejectResponse` (`0x07`)

```text
+-------------------+-----------------------+
| Tag (1b: 0x06/07) | Caller Address (32b)  |
+-------------------+-----------------------+
```

### 4.7 `CallAcceptedNotification` (`0x08`) / `CallRejectedNotification` (`0x09`) / `ConnectionError` (`0x0A`) / `CallHangup` (`0x0B`) / `CallEndedNotification` (`0x0C`)

```text
+---------------------+-----------------------+
| Tag (1b: 0x08-0x0C) | Target Address (32b)  |
+---------------------+-----------------------+
```

______________________________________________________________________

## 5. Concrete Hex Dump Packet Examples

### 5.1 `CallHangup` Packet (`0x0B` - 33 Bytes Total)

```text
0b                                                               -- Tag: 0x0B (CallHangup)
8d 96 9e ef 6e ca d3 c2 9a 3a 62 92 80 e6 86 cf                 -- Target Address (Raw 32 Bytes, Byte 0-15)
0c 3f 5d 5a 86 af f3 ca 12 02 0c 92 3a dc 6c 92                 -- Target Address (Byte 16-31)
```

### 5.2 `PeerTargetedAudio` Packet with Opus & TTL (`0x04` - 83 + N Bytes)

```text
04                                                               -- Tag: 0x04 (PeerTargetedAudio)
01 00 00 00 00 00 00 00                                         -- Sender ID: 1 (u64 LE)
fe dc ba 98 76 54 32 10                                         -- Origin Node ID (u64 LE)
8d 96 9e ef 6e ca d3 c2 9a 3a 62 92 80 e6 86 cf                 -- Target Address (Raw 32 Bytes, Byte 0-15)
0c 3f 5d 5a 86 af f3 ca 12 02 0c 92 3a dc 6c 92                 -- Target Address (Byte 16-31)
e3 b0 c4 42 98 fc 1c 14 9a fb f4 c8 99 6f b9 24                 -- Sender Address (Raw 32 Bytes, Byte 0-15)
27 ae 41 e4 64 9b 93 4c a4 95 99 1b 78 52 b8 55                 -- Sender Address (Byte 16-31)
01                                                               -- Codec ID: 0x01 (OPUS)
08                                                               -- TTL: 8 (Remaining Hop Limit)
68 24 16 00 00 01 e3 20 ...                                     -- Opus Compressed Audio Frame Bytes
```

### 5.3 `ClientTargetedAudio` Packet with Opus Codec (`0x02` - 34 + N Bytes)

```text
02                                                               -- Tag: 0x02 (ClientTargetedAudio)
8d 96 9e ef 6e ca d3 c2 9a 3a 62 92 80 e6 86 cf                 -- Target Address (Raw 32 Bytes, Byte 0-15)
0c 3f 5d 5a 86 af f3 ca 12 02 0c 92 3a dc 6c 92                 -- Target Address (Byte 16-31)
01                                                               -- Codec ID: 0x01 (OPUS)
68 24 16 00 00 01 e3 20 ...                                     -- Opus Compressed Audio Frame Bytes
```

______________________________________________________________________

## 6. Size Limits and Security

- **Maximum Packet Size**: The maximum total packet size MUST NOT exceed **1 MB (1,048,576 bytes)**. Implementations MUST drop oversized packets and log a warning.
