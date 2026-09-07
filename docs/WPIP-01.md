# WPIP-01: WPIP Architecture & Process

`draft` `optional` `author:haruki7049`

______________________________________________________________________

## Abstract

This document defines the goals, specification process, and overall system architecture for **WPIPs (web-phone Implementation Possibilities)** within the `web-phone` ecosystem.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in [RFC 2119](https://www.ietf.org/rfc/rfc2119.txt).

______________________________________________________________________

## 1. System Architecture

`web-phone` is a decentralized, real-time WebRTC audio communication platform. The system consists of three core components:

```mermaid
graph TD
    ClientA["wpclient (Client A)"] <-->|WebRTC DataChannel / Audio| Daemon1["wpdaemon (Node 1)"]
    ClientB["wpclient (Client B)"] <-->|WebRTC DataChannel / Audio| Daemon1
    Daemon1 <-->|Peer Mesh DataChannel| Daemon2["wpdaemon (Node 2)"]
    ClientC["wpclient (Client C)"] <-->|WebRTC DataChannel / Audio| Daemon2

    subgraph FFI Layer
        FFI["wpffi (C-API / Foreign Bindings)"]
    end

    ClientA -.-> FFI
    ClientB -.-> FFI
```

### Component Roles

1. **`wpclient` (Client Application)**:

   - Manages audio capture (microphone) and audio playback (speakers) with resampling.
   - Executes WebRTC SDP signaling handshakes with a `wpdaemon` node.
   - Transmits and receives real-time audio packets and call control events over WebRTC DataChannel.

1. **`wpdaemon` (Daemon Server Node)**:

   - Provides HTTP signaling endpoints (`/sdp`, `/peer/sdp`, `/addresses`).
   - Hosts a built-in UDP STUN/TURN service for NAT traversal.
   - Maintains a thread-safe registry (`ClientRegistry`) of active client connections and routes 1-to-1 audio.
   - Interconnects with other `wpdaemon` nodes via peer mesh to relay audio across nodes.

1. **`wpffi` (Core Engine & FFI)**:

   - Provides C-compatible ABI interfaces (`extern "C"` functions, header `wpffi.h`).
   - Serves as the core library for non-Rust language bindings (Python, Go, Flutter, GUIs, etc.).

______________________________________________________________________

## 2. End-to-End Connection & Streaming Sequence Diagram

The following sequence diagram details the complete end-to-end workflow from initial HTTP signaling and WebRTC DataChannel setup to interactive call control approval and bidirectional audio streaming between Client A and Client B through a `wpdaemon` node:

```mermaid
sequenceDiagram
    autonumber
    participant ClientA as wpclient (Client A)
    participant Daemon as wpdaemon
    participant ClientB as wpclient (Client B)

    %% Phase 1: Client Registration
    rect rgb(240, 248, 255)
    Note over ClientA,Daemon: Phase 1: Client Registration & DataChannel Setup
    ClientA->>Daemon: POST /sdp (SDP Offer)
    Daemon-->>ClientA: 200 OK (SDP Answer)
    Note over ClientA,Daemon: DataChannel Opened
    Daemon->>ClientA: ClientAssignment (0x01, Client ID: A, Address: AddrA)

    ClientB->>Daemon: POST /sdp (SDP Offer)
    Daemon-->>ClientB: 200 OK (SDP Answer)
    Note over ClientB,Daemon: DataChannel Opened
    Daemon->>ClientB: ClientAssignment (0x01, Client ID: B, Address: AddrB)
    end

    %% Phase 2: Call Control & Approval
    rect rgb(255, 250, 240)
    Note over ClientA,ClientB: Phase 2: Call Initiation & Approval (WPIP-08)
    ClientA->>Daemon: ClientTargetedAudio (0x02, Target: AddrB, AudioData)
    Note over Daemon: Intercept audio & check approval state
    Daemon->>ClientB: CallRequest (0x05, CallerID: A, Address: AddrA)
    Note over ClientB: Prompt User or Auto-Accept
    ClientB->>Daemon: CallAcceptResponse (0x06, CallerAddress: AddrA)
    Note over Daemon: Mark Call Approved for AddrA <-> AddrB
    Daemon->>ClientA: CallAcceptedNotification (0x08, Target: AddrB)
    end

    %% Phase 3: Bidirectional Streaming
    rect rgb(240, 255, 240)
    Note over ClientA,ClientB: Phase 3: Active Bidirectional Audio Streaming
    loop Real-time Audio Frame Transfer (PCM f32 LE 48kHz)
        ClientA->>Daemon: ClientTargetedAudio (0x02, Target: AddrB, Data)
        Daemon->>ClientB: ServerTargetedAudio (0x03, Target: AddrB, Sender: AddrA, Data)
        Note over ClientB: Play audio on Speakers

        ClientB->>Daemon: ClientTargetedAudio (0x02, Target: AddrA, Data)
        Daemon->>ClientA: ServerTargetedAudio (0x03, Target: AddrA, Sender: AddrB, Data)
        Note over ClientA: Play audio on Speakers
    end
    end
```

______________________________________________________________________

## 3. Address Identification (UserAddress)

Client identity in `web-phone` is represented by `UserAddress`:

- A 32-byte (64-character hexadecimal string) unique identifier.
- Dynamically generated from UNIX timestamps and SHA-256 hashes upon connection, or manually configured.
- Supports prefix-matching (Short ID) for address resolution and routing.

______________________________________________________________________

## 4. Design Principles

1. **Simplicity**:

   - Signaling relies on minimal HTTP POST requests (JSON/SDP).
   - Audio streaming and control events are multiplexed over a single WebRTC DataChannel.

1. **Privacy and Decentralization**:

   - Operates without centralized authentication servers. Anyone can run a `wpdaemon` node.
   - Peer mesh routing allows clients connected to different daemon nodes to communicate seamlessly.

1. **Interoperability**:

   - Language-agnostic C FFI (`wpffi`) enables embedding client and daemon capabilities into any environment.
