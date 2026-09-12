# WPIP-07: Call Control & Capacity Constraints

`draft` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an optional extension for `wpdaemon` and `wpclient` implementations to support **Call Control Protocols**, interactive call request/approval flows, and **Capacity Constraints** (e.g., maximum 2 participants limit).

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Capacity Constraints (2-Participant Limit for 1-to-1 Calls)

Implementations adopting WPIP-07 MUST enforce call capacity constraints for direct 1-to-1 calls:

- **Maximum Capacity**: A direct 1-to-1 call session targeted at a specific client `UserAddress` MUST NOT exceed **2 active participants**.
- **Group Call Exemption**: The 2-participant limit specified in WPIP-07 applies ONLY to direct 1-to-1 targeted audio streams (`ClientTargetedAudio` - `0x02`). Multi-participant group rooms operating under [WPIP-08](WPIP-08.md) (`RoomJoinRequest` / `RoomGroupAudio` - `0x10`) MUST NOT be restricted by the 2-participant limit.
- **Rejection**: If a third client attempts to connect or transmit direct audio to a 1-to-1 call session with 2 active participants, `wpdaemon` MUST respond with `ConnectionError` (`0x0A`) and drop the audio payload.

______________________________________________________________________

## 2. Interactive Call Control Flow

### 2.1 Call Request Notification (`CallRequest` - `0x05`)

- When Client A initiates audio to Client B, `wpdaemon` MUST send a `CallRequest` (`0x05`) packet to Client B.
- `wpdaemon` MUST track notification state (`mark_notified`) to prevent duplicate prompts.
- **Glare (Simultaneous Call Request) Resolution**: If Client A and Client B issue simultaneous `CallRequest` signals to each other before receiving responses, `wpdaemon` MUST automatically approve the call session by comparing the lexicographical order of their `UserAddress` strings (tie-breaker), resolving the session instantly without duplicate prompts.

### 2.2 Response Protocols

- **Call Accept (`CallAcceptResponse` - `0x06`)**: If Client B accepts the call, it sends `CallAcceptResponse` (`0x06`). `wpdaemon` marks the call as approved and sends `CallAcceptedNotification` (`0x08`) to Client A.
- **Call Reject (`CallRejectResponse` - `0x07`)**: If Client B rejects the call, it sends `CallRejectResponse` (`0x07`). `wpdaemon` marks the call as rejected, sends `CallRejectedNotification` (`0x09`) to Client A, and blocks A's audio stream.

### 2.3 Client Auto-Accept Behavior

`wpclient` MAY support an `auto_accept` configuration option:

- If `auto_accept = true`: `wpclient` MUST automatically transmit `CallAcceptResponse` upon receiving `CallRequest`.
- If `auto_accept = false`: `wpclient` MUST prompt the user interactively before responding.

### 2.4 Call Hangup & Session Termination Protocols (`CallHangup` - `0x0B` & `CallEndedNotification` - `0x0C`)

- **Call Hangup Execution**: Either participant (`wpclient`) MAY initiate call termination at any time by sending a `CallHangup` (`0x0B`) packet containing the target's 32-byte raw Ed25519 `UserAddress` to `wpdaemon`.
- **Daemon Session Cleanup**: Upon receipt of `CallHangup` (`0x0B`), `wpdaemon` MUST:
  1. Immediately revoke the active call approval state for the pair (`Caller` \<-> `Target`).
  1. Reset internal notification flags (including `mark_notified`).
  1. Relay a `CallEndedNotification` (`0x0C`) packet (containing the hangup initiator's 32-byte raw Ed25519 `UserAddress`) to the remote client.
- **Immediate State Transition**: Upon transmitting `CallHangup` or receiving `CallEndedNotification`, both `wpclient` instances MUST immediately halt audio capture, encoding, and transmission for the session, returning to **Standby Mode**.
- **DataChannel Persistence**: The WebRTC DataChannel connection between `wpclient` and `wpdaemon` MUST remain open during and after call termination, allowing clients to return to Standby Mode instantly (0ms setup latency for subsequent calls).
