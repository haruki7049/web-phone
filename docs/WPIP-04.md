# WPIP-04: DataChannel Wire Protocol

`draft` `mandatory` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines the binary wire protocol and packet encoding specifications for **`ProtocolPacket`** transmitted over WebRTC DataChannels in the `web-phone` network.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Encoding Conventions & UserAddress Binary Representation

- **Endianness**: All multi-byte integer fields (`client_id`, `origin_node`, etc.) MUST be encoded in **Little Endian**.
- **Header Tag**: The first byte (`Index 0`) of every packet MUST contain an 8-bit unsigned integer (`u8`) tag identifying the packet type.
- **`UserAddress` Ed25519 Raw Binary Representation**: When packed into DataChannel wire packets, `UserAddress` MUST be represented as **32 raw bytes** (fixed-length binary byte array `[u8; 32]`) corresponding to the client's Ed25519 Public Key, NOT as a 64-character hex ASCII string. (Human-readable 64-char hex strings are used only for text representation, logging, or JSON serialization).

______________________________________________________________________

## 2. Packet Types and Tag Mapping

| Tag (Hex) | Packet Name | Sender | Receiver | Total Fixed Length | Description |
| :---: | :--- | :---: | :---: | :---: | :--- |
| `0x01` | `ClientAssignment` | Daemon | Client | 41 Bytes | Assigns `client_id` and generated `UserAddress` |
| `0x02` | `ClientTargetedAudio` | Client | Daemon | 33 + N Bytes | Audio sent from client to target address |
| `0x03` | `ServerTargetedAudio` | Daemon | Client | 73 + N Bytes | Routed audio payload delivered from daemon to client |
| `0x04` | `PeerTargetedAudio` | Daemon | Daemon | 81 + N Bytes | Audio relay packet between inter-daemon mesh nodes |
| `0x05` | `CallRequest` | Daemon | Client | 41 Bytes | Notification of an incoming call request |
| `0x06` | `CallAcceptResponse` | Client | Daemon | 33 Bytes | User acceptance response for a call request |
| `0x07` | `CallRejectResponse` | Client | Daemon | 33 Bytes | User rejection response for a call request |
| `0x08` | `CallAcceptedNotification` | Daemon | Client | 33 Bytes | Notification to caller that target accepted call |
| `0x09` | `CallRejectedNotification` | Daemon | Client | 33 Bytes | Notification to caller that target rejected call |
| `0x0A` | `ConnectionError` | Daemon | Client | 33 Bytes | Connection rejection (e.g., maximum capacity reached) |

______________________________________________________________________

## 3. Detailed Byte Layouts

### 3.1 `ClientAssignment` (`0x01`)

```text
+-------------------+--------------------+-----------------------+
| Tag (1b: 0x01)    | Client ID (8b: u64)| UserAddress (32b raw) |
+-------------------+--------------------+-----------------------+
```

### 3.2 `ClientTargetedAudio` (`0x02`)

```text
+-------------------+-----------------------+--------------------+
| Tag (1b: 0x02)    | Target Address (32b)  | Audio Payload (...) |
+-------------------+-----------------------+--------------------+
```

### 3.3 `ServerTargetedAudio` (`0x03`)

```text
+-------------------+-----------------------+--------------------+-----------------------+--------------------+
| Tag (1b: 0x03)    | Target Address (32b)  | Sender ID (8b: u64)| Sender Address (32b)  | Audio Payload (...) |
+-------------------+-----------------------+--------------------+-----------------------+--------------------+
```

### 3.4 `PeerTargetedAudio` (`0x04`)

```text
+-------------------+--------------------+--------------------+-----------------------+-----------------------+--------------------+
| Tag (1b: 0x04)    | Sender ID (8b: u64)| Origin Node (8b)   | Target Address (32b)  | Sender Address (32b)  | Audio Payload (...) |
+-------------------+--------------------+--------------------+-----------------------+-----------------------+--------------------+
```

### 3.5 `CallRequest` (`0x05`)

```text
+-------------------+--------------------+-----------------------+
| Tag (1b: 0x05)    | Caller ID (8b: u64)| Caller Address (32b)  |
+-------------------+--------------------+-----------------------+
```

### 3.6 `CallAcceptResponse` (`0x06`) / `CallRejectResponse` (`0x07`)

```text
+-------------------+-----------------------+
| Tag (1b: 0x06/07) | Caller Address (32b)  |
+-------------------+-----------------------+
```

### 3.7 `CallAcceptedNotification` (`0x08`) / `CallRejectedNotification` (`0x09`) / `ConnectionError` (`0x0A`)

```text
+-------------------+-----------------------+
| Tag (1b: 0x08-0A) | Target Address (32b)  |
+-------------------+-----------------------+
```

______________________________________________________________________

## 4. Concrete Hex Dump Packet Examples

### 4.1 `ClientAssignment` Packet (`0x01` - 41 Bytes Total)

#### Annotated Hex Dump Example

```text
01                                                               -- Tag: 0x01 (ClientAssignment)
2a 00 00 00 00 00 00 00                                         -- Client ID: 42 (u64 Little Endian)
e3 b0 c4 42 98 fc 1c 14 9a fb f4 c8 99 6f b9 24                 -- UserAddress (Raw 32 Bytes, Byte 0-15)
27 ae 41 e4 64 9b 93 4c a4 95 99 1b 78 52 b8 55                 -- UserAddress (Byte 16-31)
```

### 4.2 `ClientTargetedAudio` Packet (`0x02` - 33 + N Bytes Total)

#### Annotated Hex Dump Example (33 Bytes Header + 8 Bytes PCM f32 audio payload)

```text
02                                                               -- Tag: 0x02 (ClientTargetedAudio)
8d 96 9e ef 6e ca d3 c2 9a 3a 62 92 80 e6 86 cf                 -- Target Address (Raw 32 Bytes, Byte 0-15)
0c 3f 5d 5a 86 af f3 ca 12 02 0c 92 3a dc 6c 92                 -- Target Address (Byte 16-31)
00 00 80 3f 00 00 00 00                                         -- Audio Data: 2x f32 LE (1.0f, 0.0f)
```

### 4.3 `CallRequest` Packet (`0x05` - 41 Bytes Total)

#### Annotated Hex Dump Example

```text
05                                                               -- Tag: 0x05 (CallRequest)
01 00 00 00 00 00 00 00                                         -- Caller ID: 1 (u64 Little Endian)
11 22 33 44 55 66 77 88 99 00 aa bb cc dd ee ff                 -- Caller Address (Raw 32 Bytes, Byte 0-15)
00 11 22 33 44 55 66 77 88 99 aa bb cc dd ee ff                 -- Caller Address (Byte 16-31)
```

### 4.4 `CallAcceptedNotification` Packet (`0x08` - 33 Bytes Total)

#### Annotated Hex Dump Example

```text
08                                                               -- Tag: 0x08 (CallAcceptedNotification)
11 22 33 44 55 66 77 88 99 00 aa bb cc dd ee ff                 -- Target Address (Raw 32 Bytes, Byte 0-15)
00 11 22 33 44 55 66 77 88 99 aa bb cc dd ee ff                 -- Target Address (Byte 16-31)
```

______________________________________________________________________

## 5. Size Limits and Security

- **Maximum Packet Size**: The maximum total packet size MUST NOT exceed **1 MB (1,048,576 bytes)**. Implementations MUST drop oversized packets and log a warning.
