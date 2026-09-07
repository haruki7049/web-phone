# WPIP-04: DataChannel Wire Protocol

`draft` `mandatory` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines the binary wire protocol and packet encoding specifications for **`ProtocolPacket`** transmitted over WebRTC DataChannels in the `web-phone` network.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Encoding Conventions

- **Endianness**: All multi-byte integer fields (`client_id`, `origin_node`, etc.) MUST be encoded in **Little Endian**.
- **Header Tag**: The first byte (`Index 0`) of every packet MUST contain an 8-bit unsigned integer (`u8`) tag identifying the packet type.

______________________________________________________________________

## 2. Packet Types and Tag Mapping

| Tag (Hex) | Packet Name | Sender | Receiver | Description |
| :---: | :--- | :---: | :---: | :--- |
| `0x01` | `ClientAssignment` | Daemon | Client | Assigns assigned `client_id` and generated `UserAddress` |
| `0x02` | `ClientTargetedAudio` | Client | Daemon | Audio data sent from client to target address |
| `0x03` | `ServerTargetedAudio` | Daemon | Client | Routed audio payload delivered from daemon to client |
| `0x04` | `PeerTargetedAudio` | Daemon | Daemon | Audio relay packet between inter-daemon mesh nodes |
| `0x05` | `CallRequest` | Daemon | Client | Notification of an incoming call request |
| `0x06` | `CallAcceptResponse` | Client | Daemon | User acceptance response for a call request |
| `0x07` | `CallRejectResponse` | Client | Daemon | User rejection response for a call request |
| `0x08` | `CallAcceptedNotification` | Daemon | Client | Notification to caller that target accepted call |
| `0x09` | `CallRejectedNotification` | Daemon | Client | Notification to caller that target rejected call |
| `0x0A` | `ConnectionError` | Daemon | Client | Connection rejection (e.g., maximum capacity reached) |

______________________________________________________________________

## 3. Detailed Byte Layouts

### 3.1 `ClientAssignment` (`0x01`)

```text
+-------------------+--------------------+-----------------------+
| Tag (1b: 0x01)    | Client ID (8b: u64)| UserAddress (32b)     |
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

## 4. Size Limits and Security

- **Maximum Packet Size**: The maximum total packet size MUST NOT exceed **1 MB (1,048,576 bytes)**. Implementations MUST drop oversized packets and log a warning.
