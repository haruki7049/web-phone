# web-phone

A high-performance, decentralized real-time WebRTC audio transmission system with SFU group call support, inter-daemon peer mesh, encrypted key store, and interactive TUI, written in Rust.

Conforms strictly to the **WPIP (web-phone Implementation Possibilities)** specifications ([`docs/WPIP-*.md`](docs/README.md)).

## Overview

`web-phone` enables decentralized real-time voice communication between CLI/TUI clients (`wpclient`) and server nodes (`wpdaemon`).
Audio is captured from the microphone via CPAL, encoded/decoded over WebRTC DataChannels, and played back on connected clients' speakers.

- **Direct 1-to-1 Calls & SFU Group Rooms**: Supports direct peer connections and Selective Forwarding Unit (SFU) multi-user audio rooms.
- **Daemon Peer Mesh**: `wpdaemon` instances interconnect with other `wpdaemon` nodes over WebRTC to bridge audio frames and client metadata seamlessly across different nodes.
- **Security & Identity**: Supports encrypted identity key storage (Argon2id + AES-256-GCM via WPIP-14) and Schnorr / Nostr public key identity authentication (WPIP-16).
- **Interactive TUI**: Feature-rich terminal user interface built with Ratatui, including audio volume meters, mute control, identity renewal, and interactive incoming call approval.

## Components

- **`wpdaemon`**: WebRTC audio signaling daemon, Selective Forwarding Unit (SFU) for group calls ([`WPIP-08`](docs/WPIP-08.md)), Keep-Alive heartbeat manager ([`WPIP-09`](docs/WPIP-09.md)), Rate Limiter ([`WPIP-15`](docs/WPIP-15.md)), and Inter-Daemon Peer Mesh node ([`WPIP-05`](docs/WPIP-05.md)). Reference implementation for [`WPIP-02`](docs/WPIP-02.md).
- **`wpclient`**: Terminal audio client supporting interactive TUI, direct 1-to-1 calls, and group audio rooms. Reference implementation for [`WPIP-03`](docs/WPIP-03.md).
- **`wpapi`**: Core WebRTC peer connection manager, protocol packet codec (`ProtocolPacket`, [`WPIP-04`](docs/WPIP-04.md)), CPAL audio engine, resampler, encrypted key store ([`WPIP-14`](docs/WPIP-14.md)), Nostr identity authenticator ([`WPIP-16`](docs/WPIP-16.md)), and C FFI bindings.

## Standards & Specifications (WPIPs)

Protocol specifications and reference behaviors are standardized in [WPIPs (web-phone Implementation Possibilities)](docs/README.md).

### Implemented WPIPs

| WPIP | Title | Category | Description |
| :--- | :--- | :---: | :--- |
| [`WPIP-01`](docs/WPIP-01.md) | WPIP Architecture & Process | Standard | WPIP definitions, process, and system architecture overview |
| [`WPIP-02`](docs/WPIP-02.md) | `wpdaemon` Core Specification | Standard | Core `wpdaemon` signaling, client registry, and audio routing |
| [`WPIP-03`](docs/WPIP-03.md) | `wpclient` Core Specification | Standard | Core `wpclient` connection lifecycle and audio streaming |
| [`WPIP-04`](docs/WPIP-04.md) | DataChannel Wire Protocol & Audio Codecs | Standard | Binary packet encoding (`ProtocolPacket`) and Opus codec support |
| [`WPIP-05`](docs/WPIP-05.md) | Inter-Daemon Peer Mesh | Optional | Multi-node daemon mesh interconnection and audio relaying |
| [`WPIP-07`](docs/WPIP-07.md) | Call Control & Capacity Constraints | Optional | Interactive call approval, rejection states, and Short ID routing |
| [`WPIP-08`](docs/WPIP-08.md) | Group Call & SFU Extension | Optional | Selective Forwarding Unit (SFU) audio routing and room identity |
| [`WPIP-09`](docs/WPIP-09.md) | Connection Keep-Alive & Health Check | Optional | Ping/Pong heartbeat packets over DataChannel for silent pruning |
| [`WPIP-14`](docs/WPIP-14.md) | Client Key Store Encryption | Optional | Argon2id & AES-256-GCM encrypted client identity key store |
| [`WPIP-15`](docs/WPIP-15.md) | Security Mechanisms & DoS Protections | Optional | Token Bucket rate limiting and authorization verification |
| [`WPIP-16`](docs/WPIP-16.md) | Secp256k1 & Nostr Identity Authentication | Optional | BIP-340 Schnorr signature authentication and Nostr (`nsec1...`) identity |

*(See [`docs/README.md`](docs/README.md) for the complete list of specifications including experimental WPIPs 11-13 and 17-21).*

## Requirements

### Linux

- ALSA development libraries: `sudo apt-get install libasound2-dev`

## Usage

### Start the Server (Daemon)

```bash
# Start server with default settings (HTTP port 15000)
cargo run -p wpdaemon

# Start server with custom HTTP signaling port
cargo run -p wpdaemon -- --port 15000

# Connect to another peer wpdaemon node to form a daemon mesh
cargo run -p wpdaemon -- --port 15001 --peer http://127.0.0.1:15000

# Alternatively, run via Podman / Containerfile
podman build -t wpdaemon .
podman run -d --name wpdaemon -p 15000:15000/tcp wpdaemon
```

### Start a Client

```bash
# Launch interactive Ratatui TUI mode (standby for calls, interactive key controls)
cargo run -p wpclient

# Stand by for incoming 1-to-1 calls in CLI mode
cargo run -p wpclient -- call

# Automatically accept incoming call requests without interactive terminal prompt
cargo run -p wpclient -- call --auto-accept # or -y

# Start a 1-to-1 call to a specific target Short ID or full SHA-256 User ID
cargo run -p wpclient -- call --to <TARGET_USER_ID_OR_SHORT_ID>

# Join an SFU group audio room
cargo run -p wpclient -- room --id <ROOM_ID>

# Use anonymous / ephemeral identity mode (in-memory temporary Ed25519 key pair)
cargo run -p wpclient -- --anonymous call

# Provide a passphrase to load/create a WPIP-14 encrypted keystore
cargo run -p wpclient -- --passphrase "my-secure-password" call

# Use Nostr private key (nsec1... or 64-char Hex) for WPIP-16 identity authentication
cargo run -p wpclient -- --nostr-key <NSEC_KEY> call

# List all registered user addresses connected to the daemon
cargo run -p wpclient -- list-addresses

# List available audio input (microphone) and output (speaker) devices
cargo run -p wpclient -- list-devices
```

### Interactive TUI Controls

When running `cargo run -p wpclient` (or `wpclient call` without a target), the interactive TUI opens:

| Key | Action | Description |
| :---: | :--- | :--- |
| `s` | **Start / Stop Standby** | Toggle standby mode to register with daemon and receive calls |
| `c` | **Call Target** | Open prompt to enter a target Short ID / User ID and initiate a call |
| `r` | **Join Room** | Open prompt to enter an SFU Room ID and join group call |
| `m` | **Toggle Mute** | Mute or unmute local microphone audio capture |
| `a` | **Toggle Auto-Accept** | Automatically approve or manually prompt for incoming calls |
| `h` / `x` | **Hangup** | Terminate current active call or leave group room |
| `u` / `k` | **Renew Identity** | Regenerate client Ed25519 keypair and reconnect to daemon |
| `l` | **Fetch Addresses** | Query daemon for list of currently registered peer addresses |
| `y` / `n` | **Accept / Reject** | Accept or reject an incoming call when prompt modal appears |
| `q` / `Ctrl+C` | **Quit** | Exit `wpclient` TUI application |

### Configuration

Configuration files are automatically stored in platform-specific default directories:

- Linux: `~/.config/wpdaemon/config.toml` (server), `~/.config/wpclient/config.toml` (client)

#### Server Configuration (`config.toml`)

```toml
ip = "127.0.0.1"
port = 15000
peers = ["http://127.0.0.1:15001"]
node_id = 1
max_connections = 1000
max_mesh_peers = 16
max_room_members = 50
```

#### Client Configuration (`config.toml`)

```toml
server_url = "http://127.0.0.1:15000" # Optional: specify full URL or hostname (overrides server_ip/server_port)
server_ip = "127.0.0.1"
server_port = 15000
stun_server = "stun:stun.l.google.com:19302"
sample_rate = 48000
channels = 1
allow_echoback = false
auto_accept = false
input_device = "Microphone" # Optional device name substring
output_device = "Speaker"   # Optional device name substring
```

## Architecture

```
┌───────────┐     WebRTC DataChannel     ┌───────────┐     Peer Mesh     ┌───────────┐     WebRTC DataChannel     ┌───────────┐
│  Client A │ ◄────────────────────────► │ Daemon 1  │ ◄───────────────► │ Daemon 2  │ ◄────────────────────────► │  Client B │
│   (mic)   │        (HTTP SDP)          │ (node 1)  │    (wpdaemon)   │ (node 2)  │        (HTTP SDP)          │ (speaker) │
└───────────┘                            └───────────┘                   └───────────┘                            └───────────┘
```

1. **WebRTC Signaling & Audio Transport**: `wpclient` connects to `wpdaemon` via HTTP SDP Offer/Answer signaling and exchanges binary audio frames (`ProtocolPacket`) over WebRTC DataChannels.
1. **SFU Group Rooms**: `wpdaemon` acts as a Selective Forwarding Unit (SFU) for multi-participant room channels (WPIP-08).
1. **Daemon Peer Mesh Interconnection**: `wpdaemon` instances mesh with peer `wpdaemon` nodes over WebRTC (WPIP-05). Audio frames and user presence metadata are relayed across the mesh, allowing clients connected to different servers to communicate seamlessly.

## License

MIT
