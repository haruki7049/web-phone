# web-phone

A real-time audio transmission system over WebTransport, written in Rust.

## Overview

web-phone enables real-time voice communication between multiple clients through a WebTransport server. Audio is captured from the microphone, transmitted over WebTransport (HTTP/3 + QUIC), and played back on connected clients' speakers.

WebTransport provides lower latency and better performance compared to WebSocket, making it ideal for real-time audio applications.

## Components

- **daemon**: WebTransport server that broadcasts audio data to all connected clients
- **client**: WebTransport client that captures microphone input and plays received audio

## Requirements

### Linux

- ALSA development libraries: `sudo apt-get install libasound2-dev`

## Usage

### Start the Server

```bash
cargo run -p daemon
```

The server listens on `https://127.0.0.1:15000` by default (uses self-signed certificate).

### Start a Client

```bash
# Start an audio call
cargo run -p client -- call

# List available audio devices
cargo run -p client -- list-devices
```

### Configuration

Configuration files are stored in platform-specific directories:

- Linux: `~/.config/web-phone-daemon/config.toml` (server), `~/.config/web-phone-client/config.toml` (client)

#### Server Configuration (`config.toml`)

```toml
ip = "127.0.0.1"
port = 15000
```

#### Client Configuration (`config.toml`)

```toml
server_ip = "127.0.0.1"
server_port = 15000
sample_rate = 48000
channels = 1
# Allow echo back (hear your own voice)
allow_echoback = false
```

## Architecture

```
┌─────────┐     Audio Data      ┌─────────┐
│ Client  │ ◄─────────────────► │ Server  │
│  (mic)  │     WebTransport    │(daemon) │
└─────────┘     (HTTP/3+QUIC)   └─────────┘
                                    │
                                    │ Broadcast
                                    ▼
                               ┌─────────┐
                               │ Client  │
                               │(speaker)│
                               └─────────┘
```

Multiple clients can connect to the server. Audio from each client is broadcast to all other connected clients.

### Room Feature

Currently, all connected clients share a single global room - audio from any client is broadcast to all other connected clients. This is referred to as the "Room" feature.

## TODO

- [ ] **Multiple Rooms**: Allow creation of multiple rooms so that audio is only broadcast to clients within the same room, not to all connected clients
  - Room creation/deletion API
  - Room join/leave commands
  - Room listing functionality
  - Optional room passwords/access control

## License

MIT
