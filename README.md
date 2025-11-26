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

## License

MIT
