# WPIP-07: Call Control & Capacity Constraints

`draft` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This specification defines an optional extension for `wpdaemon` and `wpclient` implementations to support **Call Control Protocols**, interactive call request/approval flows, and **Capacity Constraints** (e.g., maximum 2 participants limit).

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. Capacity Constraints (2-Participant Limit)

Implementations adopting WPIP-07 MUST enforce call capacity constraints:

- **Maximum Capacity**: A call session or target `UserAddress` room MUST NOT exceed **2 active participants**.
- **Rejection**: If a third client attempts to connect or transmit audio to a room with 2 or more participants, `wpdaemon` MUST respond with `ConnectionError` (`0x0A`) and drop the audio payload.

______________________________________________________________________

## 2. Interactive Call Control Flow

### 2.1 Call Request Notification (`CallRequest` - `0x05`)

- When Client A initiates audio to Client B, `wpdaemon` MUST send a `CallRequest` (`0x05`) packet to Client B.
- `wpdaemon` MUST track notification state (`mark_notified`) to prevent duplicate prompts.

### 2.2 Response Protocols

- **Call Accept (`CallAcceptResponse` - `0x06`)**: If Client B accepts the call, it sends `CallAcceptResponse` (`0x06`). `wpdaemon` marks the call as approved and sends `CallAcceptedNotification` (`0x08`) to Client A.
- **Call Reject (`CallRejectResponse` - `0x07`)**: If Client B rejects the call, it sends `CallRejectResponse` (`0x07`). `wpdaemon` marks the call as rejected, sends `CallRejectedNotification` (`0x09`) to Client A, and blocks A's audio stream.

### 2.3 Client Auto-Accept Behavior

`wpclient` MAY support an `auto_accept` configuration option:

- If `auto_accept = true`: `wpclient` MUST automatically transmit `CallAcceptResponse` upon receiving `CallRequest`.
- If `auto_accept = false`: `wpclient` MUST prompt the user interactively before responding.
