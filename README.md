# web-phone

A real-time audio transmission system over WebRTC with built-in STUN/TURN server and inter-daemon peer mesh support, written in Rust.

## Overview

`web-phone` enables real-time voice communication between clients (`wpclient`) and server nodes (`wpdaemon`).
Audio is captured from the microphone, transmitted over WebRTC DataChannels (using STUN/TURN NAT traversal), and played back on connected clients' speakers.

`wpdaemon` acts as a WebRTC audio server, STUN/TURN server, and peer mesh node that interconnects with other `wpdaemon` instances to bridge audio and client information across multiple daemon nodes.

## Components

- **`wpdaemon`**: WebRTC audio server, STUN/TURN server, and peer mesh daemon
- **`wpclient`**: WebRTC audio client CLI for making calls and listing audio devices
- **`wpffi`**: C API library (`cdylib`, `staticlib`, `rlib`) and C header (`wpffi.h`) for embedding `wpclient` into C/C++ applications

## Requirements

### Linux

- ALSA development libraries: `sudo apt-get install libasound2-dev`

## Usage

### Start the Server (Daemon)

```bash
# Start server with default settings (HTTP port 15000, STUN UDP port 3478)
cargo run -p wpdaemon

# Start server with custom ports
cargo run -p wpdaemon -- --port 15000 --stun-port 3478

# Connect to another peer wpdaemon node to form a daemon mesh
cargo run -p wpdaemon -- --port 15001 --stun-port 3479 --peer http://127.0.0.1:15000
```

### Start a Client

```bash
# Connect to wpdaemon, receive an assigned temporary SHA-256 User ID, and stand by for incoming calls
# (Prompts "Allow connection? [y/N]" when an incoming call request arrives)
cargo run -p wpclient -- call

# Automatically accept incoming call requests without interactive terminal prompt
cargo run -p wpclient -- call --auto-accept # or -y

# Join or start a 1-to-1 call with a specific target SHA-256 User ID (max 2 participants allowed)
# (Attempts by a 3rd participant to connect will be rejected with a connection error)
cargo run -p wpclient -- call --to <SHA256_USER_ID>

# List all registered wpclient temporary user IDs connected to the daemon
cargo run -p wpclient -- list-addresses

# List available audio input and output devices
cargo run -p wpclient -- list-devices
```

### Configuration

Configuration files are stored in platform-specific directories:

- Linux: `~/.config/web-phone-daemon/config.toml` (server), `~/.config/web-phone-client/config.toml` (client)

#### Server Configuration (`config.toml`)

```toml
ip = "127.0.0.1"
port = 15000
stun_port = 3478
turn_enabled = true
peers = ["http://127.0.0.1:15001"]
node_id = 1
```

#### Client Configuration (`config.toml`)

```toml
server_ip = "127.0.0.1"
server_port = 15000
user_address = "::1"
stun_server = "stun:127.0.0.1:3478"
sample_rate = 48000
channels = 1
allow_echoback = false
```

## Architecture

```
┌───────────┐     WebRTC DataChannel     ┌───────────┐     Peer Mesh     ┌───────────┐     WebRTC DataChannel     ┌───────────┐
│  Client A │ ◄────────────────────────► │ Daemon 1  │ ◄───────────────► │ Daemon 2  │ ◄────────────────────────► │  Client B │
│   (mic)   │    (HTTP SDP + UDP STUN)   │ (node 1)  │    (wpdaemon)   │ (node 2)  │    (HTTP SDP + UDP STUN)   │ (speaker) │
└───────────┘                            └───────────┘                   └───────────┘                            └───────────┘
```

1. **WebRTC Communication**: `wpclient` connects to `wpdaemon` via HTTP SDP Offer/Answer signaling and exchanges audio frames over WebRTC DataChannels.
1. **STUN/TURN Service**: `wpdaemon` runs a STUN/TURN server on UDP (port 3478 by default) for NAT traversal.
1. **Daemon Mesh Interconnection**: `wpdaemon` instances can connect to peer `wpdaemon` nodes over WebRTC. Audio and client metadata are relayed across the daemon mesh, allowing clients connected to different `wpdaemon` servers to talk to each other seamlessly.

## License

MIT
